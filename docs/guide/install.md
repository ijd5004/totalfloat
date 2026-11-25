# Install

TotalFloat runs on Linux (x86-64) and Windows (x86-64), and nothing else needs installing. A build for macOS is coming soon; until then, build from source (below).

## Install with one command

### On Windows

Open PowerShell: open Start, type `PowerShell`, and choose **Windows PowerShell**. Paste this line and press Enter:

```powershell
irm https://raw.githubusercontent.com/ijd5004/totalfloat/main/install/install.ps1 | iex
```

It downloads the latest release, checks it against the release's `SHA256SUMS`, puts `totalfloat.exe` in `%LOCALAPPDATA%\Programs\totalfloat`, and adds that folder to your `Path`. The window you ran it in can use `totalfloat` at once; other windows need to be opened again.

### On Linux

In a terminal, run:

```sh
curl -fsSL https://raw.githubusercontent.com/ijd5004/totalfloat/main/install/install.sh | sh
```

It downloads the latest release, checks it against the release's `SHA256SUMS`, and puts `totalfloat` in `~/.local/bin`. If that folder isn't on your `PATH`, it prints the line to add to your shell profile. To install somewhere else, set `TOTALFLOAT_INSTALL_DIR`, as in `curl … | TOTALFLOAT_INSTALL_DIR=/opt/bin sh`. Without `curl`, it uses `wget`.

### Upgrade, or pick a version

To upgrade, run the same command again. It replaces the installed copy with the latest release.

To install a particular version, set `TOTALFLOAT_VERSION` first. On Windows, run `$env:TOTALFLOAT_VERSION = 'v0.1.0'` before the `irm` line. On Linux, run `curl -fsSL … | TOTALFLOAT_VERSION=v0.1.0 sh`.

You can read the scripts before you run them: [install.sh](../../install/install.sh) and [install.ps1](../../install/install.ps1). If the download doesn't match `SHA256SUMS`, they stop and install nothing.

## Install by hand

Download the archive for your system from [Releases](https://github.com/ijd5004/totalfloat/releases).

### Linux

Unpack the archive, then put `totalfloat` somewhere on your `PATH`. To check the download, keep `SHA256SUMS` beside the archive and run:

```text
sha256sum --check --ignore-missing SHA256SUMS
```

### Windows

1. Download `totalfloat-vX.Y.Z-x86_64-pc-windows-msvc.zip`, and `SHA256SUMS` if you want to check it.
2. In Explorer, right-click the zip and choose **Extract All**. It proposes a folder named after the zip and puts the program in it: `Downloads\totalfloat-vX.Y.Z-x86_64-pc-windows-msvc\totalfloat.exe`. Move that folder where you want to keep it, such as `C:\Tools\totalfloat`. The v0.1.0 zip holds one more folder of the same name, with the program inside; move that inner folder instead.
3. Add the folder to your `PATH`: open Start, search for "environment variables", and choose **Edit environment variables for your account**. In the top list, **User variables for** your user name, select **Path** and choose **Edit**. Windows opens one of two boxes, depending on how many folders `Path` already holds:
   - **Edit environment variable**, a list of folders: choose **New**, paste the folder, and choose **OK**.
   - **Edit User Variable**, a single **Variable value** line with the existing value already selected. A new account's `Path` holds one folder, so it gets this box. Don't paste over the selected value: that removes the folders already in `Path`. Press End to move to the end of the line, type `;`, paste the folder after it, and choose **OK**. The value then reads like `%USERPROFILE%\AppData\Local\Microsoft\WindowsApps;C:\Tools\totalfloat`.

   Choose **OK** again to close **Environment Variables**. Open a new PowerShell or Command Prompt window afterwards: windows that were already open keep the old `PATH`.

To add the folder from PowerShell instead, run the lines below. They add it at the end of your user `Path` and keep the folders already there. If the folder is already listed, they change nothing. If you keep the folder somewhere else, change `$dir` to it.

```powershell
$dir = 'C:\Tools\totalfloat'
$entries = @([Environment]::GetEnvironmentVariable('Path', 'User') -split ';' | Where-Object { $_ })
if ($entries -notcontains $dir) { [Environment]::SetEnvironmentVariable('Path', ($entries + $dir) -join ';', 'User') }
```

These lines store `Path` as plain text, so a `%USERPROFILE%` in it is saved as the folder it stands for, such as `C:\Users\you`. Programs find the same folders either way. The dialog keeps the `%USERPROFILE%` form.

TotalFloat is a command-line program, so run it from PowerShell or Command Prompt. Double-clicking `totalfloat.exe` opens a console window that closes at once. The first double-click also shows SmartScreen's "Windows protected your PC" with "Unknown publisher", because the program isn't signed. **More info**, then **Run anyway**, lets it run. Running it from PowerShell or Command Prompt shows no such prompt.

To check the download, keep `SHA256SUMS` beside the zip. In PowerShell, this prints `True` when the zip matches:

```powershell
$zip = 'totalfloat-vX.Y.Z-x86_64-pc-windows-msvc.zip'
(Get-FileHash $zip -Algorithm SHA256).Hash -eq ((Select-String -Path SHA256SUMS -SimpleMatch $zip).Line -split '\s+')[0]
```

In Command Prompt, `certutil -hashfile FILE SHA256` prints the zip's hash to compare by eye with its line in `SHA256SUMS`.

Windows PowerShell garbles letters beyond ASCII in output it captures, such as `query --json` piped to `ConvertFrom-Json`, unless you set it to UTF-8: see [Non-ASCII text in PowerShell](query.md#non-ascii-text-in-powershell).

## Check it works

```sh
totalfloat --version
```

## Uninstall

On Linux, delete the program. If you installed it in another folder, use that folder:

```sh
rm ~/.local/bin/totalfloat
```

On Windows, delete the folder and take it out of your user `Path`. These PowerShell lines do both for the folder the one-command install uses. For a folder you chose yourself, change `$dir` to it.

```powershell
$dir = Join-Path $env:LOCALAPPDATA 'Programs\totalfloat'
Remove-Item -LiteralPath $dir -Recurse -Force
$entries = @([Environment]::GetEnvironmentVariable('Path', 'User') -split ';' | Where-Object { $_ -and $_ -ne $dir })
[Environment]::SetEnvironmentVariable('Path', $entries -join ';', 'User')
```

You can also take the folder out of `Path` in **Edit environment variables for your account**, the dialog in step 3 above.

## Build from source

To build from source on Linux, macOS or Windows, install [Rust](https://rustup.rs) and run `cargo install --path crates/cli` from a copy of the repository. That build leaves out the local browser app (`totalfloat serve`) unless you build the app first: see [ui/README.md](../../ui/README.md).
