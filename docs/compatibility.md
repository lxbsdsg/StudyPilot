# Compatibility Checklist

Run this checklist against a copied data directory before a release. An
automated test is not proof of live platform availability.

| Area | Requirement | Status |
| --- | --- | --- |
| Credentials | Read the existing encrypted account secret | Python and Rust regression tests |
| Sessions | Preserve login cookies and expiry metadata | Python and Rust regression tests |
| Tasks | Preserve task status, deadlines, and completed history | Python and Rust regression tests |
| Reminders | Preserve repeat rules and enabled state | Python and Rust regression tests |
| Settings | Preserve settings stored in SQLite | Python and Rust regression tests |
| Migration | Back up the database before an existing schema changes | Automated test |
| Batch refresh | Refresh one account without overwriting another | Rust tests; live browser QA needed |
| WELearn | Exercise SSO, course, and unit reading with a real account | Needs live account |
| U campus | Exercise login, course, and resource reading with a real account | Needs live account |
| Chaoxing | Exercise login, course, and homework reading with a real account | Needs live account |
| Pigai | Verify browser login, work list, and current essay lookup | Offline fixtures only |
| Browser mode | Check frontend assets, API responses, and narrow layouts | Automated build; browser QA needed |
| Desktop | Verify the packaged application, tray, opener, and notifications | Compile check only |
| Concurrent use | Do not run Python and Rust writers against the same database together | Required operating rule |
