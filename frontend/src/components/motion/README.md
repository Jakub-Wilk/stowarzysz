# Motion

One small system for animation. Tokens and keyframes live in `src/index.css` ("Motion system");
the building blocks live here. Reduced motion is handled once, globally, in `index.css`, so new
code needs nothing per component.

## Vocabulary

- Durations: `--duration-fast|base|slow`. Easings: `--ease-out-soft`, `--ease-spring`.
- Utilities: `animate-fade-up`, `animate-pop-in`, `animate-shimmer` (skeletons), `animate-glow`
  (attention pulse), `animate-bump` (feedback), `animate-wiggle` (errors), `animate-flash`
  (live-update highlight), `stagger` (delay from `--i`, capped).
- Primitives: `Stagger` (lists), `Reveal` (one block), `PageTransition` (layouts),
  `Skeleton` (`components/ui`), `useCountUp(n)`, `useFlashOnChange(key)`.

## Rules

- Animate `transform` and `opacity` only. Metallic text uses `background-clip: text`, so never
  animate `color` on it.
- Colors/glows come from theme tokens (`themes/*.css`), never hard-coded, so themes stay swappable.
- Use these primitives rather than a second approach. No animation library unless exit or layout
  animations are really needed; then pick one for the whole app and update this file.

## Checklist for a new view

1. Layout wraps its outlet in `PageTransition` (done for `SubPageLayout`/`ManageLayout`; the tab
   shell is animated by `SwipePager` instead).
2. Lists render through `Stagger`.
3. Loading state is `Skeleton` rows, not text.
4. Live (SSE) changes use `useFlashOnChange` + `animate-flash`.
5. A new poll kind (`features/voting/kinds/`) animates its result entrance; `ScoreDial` is the
   reference.
