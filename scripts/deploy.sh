#!/usr/bin/env bash
# Deploy the production stack: pull master, back up the database, rebuild, launch, verify, ensure cron.
#
#   scripts/deploy.sh              # full deploy
#   scripts/deploy.sh --dry-run    # preflight + print every step, change nothing
#   scripts/deploy.sh --cron-only  # only (re)install and verify the poll-deadline cron job
#
# master is production. Every step aborts the deploy on failure. Nothing is stopped before the new images
# are built, and a failed launch rolls the code back to the previous commit. The database is never
# restored automatically (that would discard data written since the dump): the script prints the commands.

set -Eeuo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
COMPOSE_FILE=docker-compose.prod.yml
ENV_FILE=.env.prod
BRANCH=master
DB_USER=stowarzysz
DB_NAME=stowarzysz
DB_VOLUME=stowarzysz-prod_pgdata # compose project name (see `name:` in the compose file) + volume
BACKUP_DIR=$ROOT/backups
BACKUPS_KEPT=3
LOG_DIR=$ROOT/logs
LOGS_KEPT=20
CRON_MARK='# stowarzysz-deadlines'
MIN_FREE_MB=${MIN_FREE_MB:-1024}
REQUIRED_ENV=(POSTGRES_PASSWORD SECRET_KEY SECRET_SANTA_KEY ALLOWED_HOSTS FRONTEND_URL)

DRY_RUN=0
CRON_ONLY=0
CURRENT_STEP=startup
ROLLBACK_ARMED=0
STACK_TOUCHED=0
PREV_SHA=${DEPLOY_PREV_SHA:-}
NEW_SHA=
BACKUP_FILE=

log() { printf '%s %s\n' "$(date -u +%FT%TZ)" "$*"; }
step() {
  CURRENT_STEP=$*
  log "==> $*"
}
die() {
  log "ERROR: $*"
  on_fail 1
}
run() {
  if ((DRY_RUN)); then log "[dry-run] $*"; else "$@"; fi
}
dc() { docker compose -f "$ROOT/$COMPOSE_FILE" --env-file "$ROOT/$ENV_FILE" "$@"; }

# Value of KEY in .env.prod: last assignment wins, optional quotes and CRs stripped.
env_get() {
  local v
  v=$(grep -E "^[[:space:]]*$1=" "$ROOT/$ENV_FILE" | tail -n1 | cut -d= -f2- | tr -d '\r') || true
  v=${v%\"} v=${v#\"} v=${v%\'} v=${v#\'}
  printf '%s' "$v"
}

# Delete the oldest files matching PATTERN in DIR until at most KEEP remain. Names embed a UTC timestamp,
# so a C-locale glob sort is chronological (never trust mtime).
prune_oldest() {
  local dir=$1 pattern=$2 keep=$3 files
  local LC_ALL=C
  shopt -s nullglob
  # shellcheck disable=SC2206 # the glob is the point
  files=("$dir"/$pattern)
  shopt -u nullglob
  while ((${#files[@]} > keep)); do
    rm -f -- "${files[0]}"
    files=("${files[@]:1}")
  done
}

latest_backup() {
  local files
  shopt -s nullglob
  files=("$BACKUP_DIR"/db-*.dump)
  shopt -u nullglob
  ((${#files[@]})) && printf '%s' "${files[${#files[@]} - 1]}"
  return 0
}

restore_hint() {
  local dump
  dump=$(latest_backup)
  if [[ -z $dump ]]; then
    log "No database snapshot exists in $BACKUP_DIR."
    return
  fi
  cat <<EOF

If the previous version will not start because a migration already ran, restore the database from the
snapshot taken before this deploy ($dump). This DISCARDS everything written since then:

  cd $ROOT
  docker compose -f $COMPOSE_FILE --env-file $ENV_FILE stop backend web
  docker compose -f $COMPOSE_FILE --env-file $ENV_FILE exec -T db psql -U $DB_USER -d postgres \\
    -c 'DROP DATABASE $DB_NAME WITH (FORCE)' -c 'CREATE DATABASE $DB_NAME OWNER $DB_USER'
  docker compose -f $COMPOSE_FILE --env-file $ENV_FILE exec -T db pg_restore -U $DB_USER -d $DB_NAME --no-owner < $dump
  docker compose -f $COMPOSE_FILE --env-file $ENV_FILE up -d

EOF
}

# Code-only rollback; see the header for why the database is left alone.
rollback() {
  local ok=0
  log "Rolling back code to ${PREV_SHA:0:9}"
  if [[ -n $PREV_SHA && $(git -C "$ROOT" rev-parse HEAD) != "$PREV_SHA" ]]; then
    git -C "$ROOT" reset --hard "$PREV_SHA" || {
      log "git reset failed; the working tree is still at the new commit"
      restore_hint
      return
    }
  fi
  if ((STACK_TOUCHED)); then
    if [[ $PREV_SHA != "$NEW_SHA" ]]; then
      # Best effort: health_check below decides whether the rollback worked.
      if dc build; then dc up -d --remove-orphans --wait --wait-timeout 180 || true; fi
    fi
    health_check && ok=1
  else
    ok=1 # the running stack was never touched
  fi
  if ((ok && !STACK_TOUCHED)); then
    log "Rolled back the working tree; the running stack was never touched."
  elif ((ok)); then
    log "Rollback complete: the previous version is serving."
  else
    log "Rollback FAILED: the previous version is not healthy either."
    dc ps || true
    restore_hint
  fi
}

on_fail() {
  local status=$1
  trap - ERR INT TERM
  set +eE
  log "FAILED (exit $status) during: $CURRENT_STEP"
  ((ROLLBACK_ARMED && !DRY_RUN)) && rollback
  exit "$status"
}

# End to end through nginx. Django rejects hosts outside ALLOWED_HOSTS, so send the first allowed one.
health_check() {
  local host port body
  host=$(env_get ALLOWED_HOSTS | cut -d, -f1 | tr -d ' ')
  host=${host#.}
  [[ -z $host || $host == '*' ]] && host=localhost
  port=$(env_get WEB_PORT)
  for _ in $(seq 1 15); do
    body=$(curl -fsS --max-time 5 -H "Host: $host" "http://127.0.0.1:${port:-8080}/api/health/" 2>/dev/null || true)
    grep -Eq '"status"[[:space:]]*:[[:space:]]*"ok"' <<<"$body" && return 0
    sleep 2
  done
  return 1
}

preflight() {
  step "Preflight"
  local cmd key free_mb perms
  for cmd in git docker curl flock crontab pgrep; do
    command -v "$cmd" >/dev/null || die "missing required command: $cmd"
  done
  docker compose version >/dev/null 2>&1 || die "the docker compose plugin is not available"
  docker info >/dev/null 2>&1 || die "cannot talk to the docker daemon (is it running, and are you in the docker group?)"
  [[ -f $ROOT/$ENV_FILE ]] || die "$ENV_FILE not found (cp .env.prod.example .env.prod and fill it in)"
  perms=$(stat -c %a "$ROOT/$ENV_FILE")
  [[ $perms == 600 || $perms == 400 ]] || die "$ENV_FILE holds secrets but has mode $perms: chmod 600 $ENV_FILE"
  for key in "${REQUIRED_ENV[@]}"; do
    [[ -n $(env_get "$key") ]] || die "$key is not set in $ENV_FILE"
  done
  if [[ -z $(env_get VAPID_PUBLIC_KEY) || -z $(env_get VAPID_PRIVATE_KEY) ]]; then
    log "WARNING: VAPID keys are not set in $ENV_FILE, so push notifications are disabled"
  fi
  dc config -q || die "$COMPOSE_FILE / $ENV_FILE do not validate"
  if ((!CRON_ONLY)); then
    [[ $(git -C "$ROOT" rev-parse --abbrev-ref HEAD) == "$BRANCH" ]] || die "not on $BRANCH"
    [[ -z $(git -C "$ROOT" status --porcelain --untracked-files=no) ]] ||
      die "the working tree has uncommitted changes to tracked files; refusing to deploy over them"
    free_mb=$(($(df --output=avail -k "$ROOT" | tail -n1) / 1024))
    ((free_mb >= MIN_FREE_MB)) || die "only ${free_mb} MB free (need $MIN_FREE_MB); free some space first"
  fi
}

pull() {
  step "Pull $BRANCH"
  PREV_SHA=$(git -C "$ROOT" rev-parse HEAD)
  git -C "$ROOT" fetch origin --prune || die "git fetch failed"
  NEW_SHA=$(git -C "$ROOT" rev-parse "origin/$BRANCH")
  if ((DRY_RUN)); then
    log "[dry-run] would fast-forward ${PREV_SHA:0:9} -> ${NEW_SHA:0:9}"
    return
  fi
  ROLLBACK_ARMED=1 # from here on a failure puts the working tree back
  git -C "$ROOT" merge --ff-only "origin/$BRANCH" || die "cannot fast-forward to origin/$BRANCH (diverged history?)"
  log "${PREV_SHA:0:9} -> $(git -C "$ROOT" rev-parse --short=9 HEAD)"
  # bash reads a script lazily, so if this very file changed, restart on the new version.
  if ! git -C "$ROOT" diff --quiet "$PREV_SHA" HEAD -- scripts/deploy.sh; then
    log "deploy.sh itself changed; restarting on the new version"
    DEPLOY_REEXEC=1 DEPLOY_PREV_SHA=$PREV_SHA exec "$ROOT/scripts/deploy.sh" "${ORIG_ARGS[@]}"
  fi
}

backup() {
  step "Back up the database"
  local partial stamp running
  running=$(dc ps --status running --services)
  if ! grep -qx db <<<"$running"; then
    if docker volume inspect "$DB_VOLUME" >/dev/null 2>&1; then
      log "db is not running but its volume exists; starting it for the dump"
      run dc up -d --wait db
    else
      log "No database volume yet (first deploy): nothing to back up"
      return
    fi
  fi
  if ((DRY_RUN)); then
    log "[dry-run] would pg_dump -Fc into $BACKUP_DIR, verify it, keep the newest $BACKUPS_KEPT"
    return
  fi
  mkdir -p "$BACKUP_DIR" && chmod 700 "$BACKUP_DIR"
  rm -f -- "$BACKUP_DIR"/.partial-* # leftovers of an interrupted run; never counted as snapshots
  stamp=$(date -u +%Y%m%dT%H%M%SZ)
  partial=$BACKUP_DIR/.partial-$stamp
  # `-Fc` is compressed and checkable with pg_restore --list; the dump is a consistent snapshot.
  (umask 077 && dc exec -T db pg_dump -U "$DB_USER" -Fc "$DB_NAME" >"$partial") || {
    rm -f -- "$partial"
    die "pg_dump failed; existing snapshots are untouched"
  }
  if [[ ! -s $partial ]] || ! dc exec -T db pg_restore --list <"$partial" >/dev/null; then
    rm -f -- "$partial"
    die "the new dump is empty or unreadable; existing snapshots are untouched"
  fi
  sync "$partial"
  # Only a verified dump may displace an old one: make room for it, then move it in atomically.
  prune_oldest "$BACKUP_DIR" 'db-*.dump' $((BACKUPS_KEPT - 1))
  BACKUP_FILE=$BACKUP_DIR/db-$stamp-${PREV_SHA:0:9}.dump
  mv -- "$partial" "$BACKUP_FILE"
  log "Snapshot: $BACKUP_FILE ($(du -h "$BACKUP_FILE" | cut -f1))"
}

build() {
  step "Build images (the running stack keeps serving)"
  run dc build
}

launch() {
  step "Launch"
  STACK_TOUCHED=1
  # Migrations run when the backend starts; a failing one keeps it unhealthy so --wait fails.
  run dc up -d --remove-orphans --wait --wait-timeout 180
  if ((DRY_RUN)); then
    log "[dry-run] would poll /api/health/ through nginx"
    return
  fi
  health_check || die "the stack is up but /api/health/ does not answer through nginx"
  log "Healthy"
}

install_cron() {
  step "Cron job for poll deadlines"
  local docker_bin flock_bin cmd line current new installed
  docker_bin=$(command -v docker) flock_bin=$(command -v flock)
  [[ $ROOT =~ ^[A-Za-z0-9_./-]+$ ]] || die "the repo path '$ROOT' has characters that are unsafe in a crontab"
  pgrep -x cron >/dev/null || pgrep -x crond >/dev/null ||
    die "the cron daemon is not running (Debian/Ubuntu: sudo service cron start; WSL does not start it by itself)"
  # Cron runs with a minimal PATH and /bin/sh, hence absolute paths. flock stops runs piling up.
  cmd="cd $ROOT && $flock_bin -n $ROOT/.deadlines.lock $docker_bin compose -f $COMPOSE_FILE --env-file $ENV_FILE exec -T backend python manage.py process_poll_deadlines >> $LOG_DIR/deadlines.log 2>&1"
  line="*/5 * * * * $cmd $CRON_MARK"
  current=$(crontab -l 2>/dev/null || true)
  new=$(printf '%s\n' "$current" | grep -vF "$CRON_MARK" || true)
  new=$(printf '%s\n%s\n' "$new" "$line" | sed '/./,$!d')
  if [[ $current == "$new" ]]; then
    log "crontab entry already up to date"
  elif ((DRY_RUN)); then
    log "[dry-run] would install: $line"
    return
  else
    mkdir -p "$BACKUP_DIR" "$LOG_DIR"
    printf '%s\n' "$current" >"$BACKUP_DIR/crontab-before-deploy.txt"
    printf '%s\n' "$new" | crontab -
  fi
  ((DRY_RUN)) && return
  installed=$(crontab -l)
  [[ $(grep -cF "$CRON_MARK" <<<"$installed") -eq 1 ]] || die "expected exactly one '$CRON_MARK' line in the crontab"
  grep -qxF "$line" <<<"$installed" || die "the installed crontab line differs from what was written"
  # Registered is not working: run the exact command cron will run (retry once if a cron run holds the lock).
  mkdir -p "$LOG_DIR"
  sh -c "$cmd" || { sleep 5 && sh -c "$cmd"; } || die "the cron command fails when run by hand; see $LOG_DIR/deadlines.log"
  log "Cron job installed and verified: $(tail -n1 "$LOG_DIR/deadlines.log")"
}

setup_logging() {
  ((DRY_RUN)) && return
  mkdir -p "$LOG_DIR"
  if [[ -z ${DEPLOY_REEXEC:-} ]]; then
    prune_oldest "$LOG_DIR" 'deploy-*.log' $((LOGS_KEPT - 1))
    exec > >(tee -a "$LOG_DIR/deploy-$(date -u +%Y%m%dT%H%M%SZ).log") 2>&1
  fi
}

acquire_lock() {
  # A re-exec after a self-update inherits the lock (fd 9) from the process it replaces.
  ((DRY_RUN)) || [[ -n ${DEPLOY_REEXEC:-} ]] && return
  exec 9>"$ROOT/.deploy.lock"
  flock -n 9 || die "another deploy is already running"
}

summary() {
  step "Done"
  log "Version:  ${PREV_SHA:0:9} -> $(git -C "$ROOT" rev-parse --short=9 HEAD)"
  log "Snapshot: ${BACKUP_FILE:-none}"
  dc ps
}

main() {
  ORIG_ARGS=("$@")
  local arg
  for arg in "$@"; do
    case $arg in
      --dry-run) DRY_RUN=1 ;;
      --cron-only) CRON_ONLY=1 ;;
      -h | --help)
        sed -n '2,/^$/{s/^# \{0,1\}//;/^$/d;p}' "${BASH_SOURCE[0]}"
        exit 0
        ;;
      *)
        echo "unknown option: $arg (see --help)" >&2
        exit 2
        ;;
    esac
  done
  cd "$ROOT"
  setup_logging
  acquire_lock
  trap 'on_fail $?' ERR
  trap 'on_fail 130' INT TERM
  ((DRY_RUN)) && log "DRY RUN: nothing will be changed"

  preflight
  if ((CRON_ONLY)); then
    install_cron
    return
  fi
  if [[ -z ${DEPLOY_REEXEC:-} ]]; then
    pull
  else
    ROLLBACK_ARMED=1
    NEW_SHA=$(git rev-parse HEAD)
  fi
  dc config -q || die "the updated $COMPOSE_FILE does not validate"
  backup
  build
  launch
  ROLLBACK_ARMED=0 # the new version is healthy: a cron problem must not undo a good deploy
  install_cron
  summary
}

# Sourcing the file (for tests) defines the functions without deploying. `exit` keeps bash from reading on
# past this line if the file is replaced while running.
if [[ ${BASH_SOURCE[0]} == "$0" ]]; then
  main "$@"
  exit $?
fi
