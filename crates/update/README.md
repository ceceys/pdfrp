# migez-update

Pdfrp's update check (1.1.0). No UI; `migez-ui-egui` (`src/platform/updates.rs`) runs it on a
thread of its own and shows the result.

1. `check` asks `https://api.github.com/repos/ceceys/pdfrp/releases/latest` and compares the
   release tag (`v1.2.0`) with the running version (semver). A draft or pre-release is never
   offered. A newer release must carry `Pdfrp Kurulum X.Y.Z.exe` (GitHub may store it as
   `Pdfrp.Kurulum.X.Y.Z.exe`) and `SHA256SUMS.txt`.
2. `download` fetches `SHA256SUMS.txt`, then the installer into a new folder under %TEMP%, and checks
   the installer's SHA-256 against the list (and against GitHub's own asset digest when GitHub gives
   one). A file that differs is deleted and never run. The checked file stays open, shared for
   reading only, until it is started, so it can't change in between.
3. `Verified::launch` starts it with `installer_args`: `/NORESTART /MIGEZYENIDENAC=1`, plus
   `/CURRENTUSER` or `/ALLUSERS` for the copy being replaced. The app does so only as it exits
   through the quit Update now asked for; a cancelled quit calls it off. The install is never
   silent: the installer's wizard shows, skips the pages an update doesn't need, waits on its
   "Ready to Install" page for the user, asks before it closes a program still running, and its
   last page starts Pdfrp again.
4. `clean_old_downloads` (at start) removes what earlier updates left in %TEMP%: only folders named
   as `download` names them, holding nothing but installers, an hour old or more.

What leaves the computer: HTTPS requests to `api.github.com`, `github.com` (only this repository's
release downloads, `/ceceys/pdfrp/releases/download/`) and GitHub's file hosts
(`objects.githubusercontent.com`, `release-assets.githubusercontent.com`), with a
`User-Agent: Pdfrp/<version>` header and nothing else of the app's. Plain HTTP, other hosts,
other paths on `github.com`, ports, user names in addresses and redirects elsewhere are refused.

Testing without the network: `MIGEZ_UPDATE_URL` names a local release JSON (a path or `file:///`
URL, whose assets are local too) or `status:404` / `status:403` / `status:429`; an `https://` value
is refused. `cargo test -p migez-update` covers versions, addresses, the JSON, SHA256SUMS.txt, the
download and its SHA-256, and starting a file held open.
