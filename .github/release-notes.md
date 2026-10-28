<!--
  The body of every release. `ship` in `.github/workflows/ci.yml` fills in
  `{version}` and attaches the files.

  English, like everything else that leaves this repository: whoever downloads
  a build is not assumed to read ours.
-->

## Downloads

| File | What it is |
|---|---|
| `sheetsift-{version}-windows-setup.zip` | Windows. Unzip it and run the installer inside. |
| `sheetsift-{version}-windows-portable.zip` | Windows, nothing installed. Unzip it and run `sheetsift.exe` from wherever you put it. |
| `sheetsift-{version}-macos.dmg` | macOS, both Apple silicon and Intel. |

The app draws in Microsoft's WebView2 runtime, which Windows 10 and 11 normally
already carry. **The installer brings it if it is missing; the no-install zip
cannot.** On a Windows that was shipped without it — LTSC, N and KN editions,
stripped images — `sheetsift.exe` opens an empty window and says nothing useful.
If that happens, either use the installer or install the runtime from Microsoft:

<https://developer.microsoft.com/microsoft-edge/webview2/>


## On macOS, one command first

The Mac app is **not signed**. macOS will say it is damaged and refuse to open
it. That is the missing signature, not a broken download.

Open the dmg, drag the app to Applications, and then run this once:

```
xattr -cr /Applications/sheetsift.app
```

After that it opens normally.
