//! Every screen's URL (the React frontend's paths, so links and notification URLs keep
//! working). Module pages are placeholders until their own plans land.

use dioxus::prelude::*;

use crate::home::Home;
use crate::layout::{Screen, SignedIn, Superuser};
use crate::pages::{
    Activate, Diagnostics, EntryDetail, EntryEdit, EntryNew, Ledger, LedgerNew, Login, Manage,
    ManageSanta, ManageUser, ManageUserNew, ManageUsers, NotFound, PactDetail, PactNew, Pacts,
    PollDetail, PollNew, SecretSanta, Voting,
};

#[derive(Debug, Clone, PartialEq, Routable)]
#[rustfmt::skip]
#[allow(clippy::enum_variant_names)]
pub enum Route {
    #[route("/login")]
    Login {},
    #[route("/activate/:token")]
    Activate { token: String },
    /// Unlinked QA page: build, session, events, push, updates.
    #[route("/diagnostyka")]
    Diagnostics {},

    #[layout(SignedIn)]
        #[layout(Screen)]
            #[route("/")]
            Home {},
            #[route("/voting")]
            Voting {},
            #[route("/voting/new")]
            PollNew {},
            #[route("/voting/:id")]
            PollDetail { id: i64 },
            #[route("/pacts")]
            Pacts {},
            #[route("/pacts/new")]
            PactNew {},
            #[route("/pacts/:id")]
            PactDetail { id: i64 },
            #[route("/ledger")]
            Ledger {},
            #[route("/ledger/new")]
            LedgerNew {},
            #[route("/ledger/new/:kind")]
            EntryNew { kind: String },
            #[route("/ledger/:id/edit")]
            EntryEdit { id: i64 },
            #[route("/ledger/:id")]
            EntryDetail { id: i64 },
            #[route("/secret-santa")]
            SecretSanta {},
            #[layout(Superuser)]
                #[route("/manage")]
                Manage {},
                #[route("/manage/users")]
                ManageUsers {},
                #[route("/manage/users/new")]
                ManageUserNew {},
                #[route("/manage/users/:id")]
                ManageUser { id: i64 },
                #[route("/manage/secret-santa")]
                ManageSanta {},
            #[end_layout]
        #[end_layout]
    #[end_layout]

    #[route("/:..segments")]
    NotFound { segments: Vec<String> },
}
