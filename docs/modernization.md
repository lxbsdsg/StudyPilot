# Modernization Status

This branch migrates StudyPilot to a Tauri desktop application while retaining
the existing Python application and its database format.

## Compatibility contract

- The legacy database stays at `%USERPROFILE%\.studypilot\studypilot.db`.
- Existing tables and credentials remain readable by both applications.
- Schema changes are additive and applied through ordered migrations.
- A database backup is created before a migration changes an existing file.
- The Python and Rust implementations must agree on API fixture results.
- The old launcher remains available until the new application passes the
  regression suite.

The existing Chrome or Edge installation is used for the browser login adapters.
Applications must not launch a second service against the same database.

## Implemented architecture

| Layer | Implementation | Status |
| --- | --- | --- |
| Desktop shell | Tauri 2, Rust 2024 | Implemented; development build live-tested, packaging still needs QA |
| Desktop IPC | Typed Tauri commands backed by `studypilot-core` | Implemented; checked by compilation |
| Browser compatibility | Axum local HTTP API and OpenAPI-compatible JSON responses | Implemented; needs browser QA |
| Frontend | React 19, TypeScript, TanStack Query, Vite | Implemented; unit tests and production build |
| Persistence | SQLite, ordered additive migrations, pre-migration backup | Implemented; regression tests |
| Reminders | Claim-before-notify, repeat rules, 20-second desktop scan | Implemented; live notification still needs QA |
| Legacy Python | Original entrypoint, adapters, local web UI, and tests | Retained |

The shared Rust core supplies accounts, tasks, credentials, reminders, workspace
data, and the Pigai adapter to both the Axum server and the Tauri commands. U
campus, WELearn, and Chaoxing scraping remain in the legacy Python application
until their authentication state can be migrated safely.

The Tauri application uses the tray to keep reminder delivery alive after the
main window is hidden. It does not launch a second local service against the
database. The legacy Python and new Rust processes must not be used to write the
same database simultaneously.

## Acceptance gates

- Legacy credentials, sessions, tasks, completion history, and reminders survive
  a migration and can be read by the new application.
- Frontend API contracts and reminder calculations have regression tests.
- Every platform has deterministic offline fixtures; live verification is
  recorded separately and never inferred from a successful compile.
- Desktop and mobile layouts have no horizontal overflow or overlapping text.
- The legacy application remains runnable and its test suite stays green.

## Verification and remaining work

Automated checks currently cover the Python migration and compatibility suite,
Rust core and API behavior, reminder claim rules, frontend API mapping, reminder
event handling, and Pigai offline fixtures. Rust core and Tauri crates compile;
the React client has unit tests and a production build.

The following are not yet evidence of live support:

- U campus, WELearn, and Chaoxing course or homework polling with a real account.
- A Pigai login and complete live essay lookup against the current website.
- A packaged Tauri installation, tray interaction, and Windows notification
  delivery in a clean user environment.
- Packaged installation and a clean-environment upgrade.

Run the desktop development application with `run_tauri.bat`, or from
the repository root with `npm run desktop:dev`. Run the browser compatibility
application with `run_rust.bat`. Use a copied
`STUDYPILOT_DATA_DIR` for acceptance testing.
