<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/banner-dark.svg">
  <img alt="Ezoo Download: parallel download segments filling in" src="assets/banner-light.svg" width="100%">
</picture>

**Multi-connection, resumable downloads for Windows and Linux.**<br>
Free and open source. No ads. No accounts.

[English](README.md) | [العربية](README.ar.md)

[![Release](https://img.shields.io/github/v/release/kerlif/ezoo-download?label=release&color=2ba59a)](https://github.com/kerlif/ezoo-download/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/kerlif/ezoo-download/total?label=downloads&color=0d1b2a)](https://github.com/kerlif/ezoo-download/releases)
[![License](https://img.shields.io/github/license/kerlif/ezoo-download?color=f5a524)](#license)
![Platforms](https://img.shields.io/badge/platforms-Windows%20%7C%20Linux-0d1b2a)
![Languages](https://img.shields.io/badge/interface-8%20languages-2ba59a)

<br>

<a href="https://github.com/kerlif/ezoo-download/releases/latest"><img src="assets/icons/windows.svg" width="18" height="18" alt=""> Windows installer</a> &nbsp;&nbsp;&nbsp;
<a href="https://github.com/kerlif/ezoo-download/releases/latest"><img src="assets/icons/box.svg" width="18" height="18" alt=""> Linux packages</a> &nbsp;&nbsp;&nbsp;
<a href="https://github.com/kerlif/ezoo-download/releases/latest"><img src="assets/icons/browser.svg" width="18" height="18" alt=""> Browser extension</a> &nbsp;&nbsp;&nbsp;
<a href="https://kerlif.github.io/ezoo-download/"><img src="assets/icons/globe.svg" width="18" height="18" alt=""> Website</a>

</div>

<br>

## Why Ezoo Download

| | Feature | What it does |
|:-:|---|---|
| <img src="assets/icons/segments.svg" width="22" height="22" alt=""> | **Adaptive connections** | Splits each file across up to 32 connections and adjusts their number to what your server and connection can handle. It adds connections while throughput keeps rising. |
| <img src="assets/icons/resume.svg" width="22" height="22" alt=""> | **Real resume** | Continues after a crash, a closed app, or a dropped connection. Progress is saved locally and checked when the download resumes. |
| <img src="assets/icons/video.svg" width="22" height="22" alt=""> | **Video and audio streams** | Reads HLS and DASH playlists and shows the duration, the available qualities, and an estimated size marked with "≈". |
| <img src="assets/icons/browser.svg" width="22" height="22" alt=""> | **Browser capture** | The Chrome and Firefox extension sends a link to the app in one click. |
| <img src="assets/icons/folder.svg" width="22" height="22" alt=""> | **Automatic folders** | Sorts finished files into folders by type. |
| <img src="assets/icons/schedule.svg" width="22" height="22" alt=""> | **Scheduling and queues** | Starts downloads at a set time and can shut the computer down when the queue is done. |
| <img src="assets/icons/speed.svg" width="22" height="22" alt=""> | **Speed limits** | A global limit, or a limit for a single download. |
| <img src="assets/icons/tray.svg" width="22" height="22" alt=""> | **Background mode** | Stays in the system tray after the window closes. |
| <img src="assets/icons/globe.svg" width="22" height="22" alt=""> | **Eight languages** | English, Arabic (right to left), French, Spanish, German, Portuguese, Turkish, and Russian. |
| <img src="assets/icons/moon.svg" width="22" height="22" alt=""> | **Dark mode** | Nine built-in themes, including Dark and Black — pick one in Settings. |
| <img src="assets/icons/shield.svg" width="22" height="22" alt=""> | **Private by design** | No accounts and no analytics. The only automatic network requests are the update check and a daily tool update check, both documented below. |
| <img src="assets/icons/ban.svg" width="22" height="22" alt=""> | **No ads** | No banners and no donation prompts. |

> [!NOTE]
> In this version, choosing a quality in the capture window is for information only. The download uses the default quality.

## How it works

1. **Paste a link.** The app picks it up from the clipboard, or you can add it yourself.
2. **Split and download.** The file is divided across several connections, and their number adapts while the download runs.
3. **Stop and resume.** Pause, close the app, or lose the network. The download continues from where it stopped.

## Screenshots

| | |
|:-:|:-:|
| <img src="docs/screenshots/04-main-en-light.png" width="100%" alt="Main window, light theme"> | <img src="docs/screenshots/05-main-en-dark.png" width="100%" alt="Main window, dark theme"> |
| Light theme | Dark theme |
| <img src="docs/screenshots/07-add-dialog-dash-ar.png" width="100%" alt="Capture dialog for a DASH stream"> | <img src="docs/screenshots/08-details-ar.png" width="100%" alt="Download details with a segment bar per connection"> |
| Capture dialog: duration, qualities, estimated size | Details: segments per connection |

## Installation

<details>
<summary><b>Windows</b></summary>

1. Download the installer from the [latest release](https://github.com/kerlif/ezoo-download/releases/latest).
2. Run it. It installs for your user account and does not need administrator rights.

> [!WARNING]
> Windows SmartScreen may show a warning, because the installer is not signed with a code-signing certificate yet. Check the file against `SHA256SUMS-windows.txt` before you run it, or review the source code in this repository.

</details>

<details>
<summary><b>Linux</b></summary>

Pick the format that fits your distribution:

```bash
# Debian and Ubuntu
sudo dpkg -i EzooDownload_*_amd64.deb

# Fedora, RHEL, openSUSE
sudo dnf install ./ezoo-download-*.x86_64.rpm

# Any distribution, no installation needed
chmod +x EzooDownload-*-x86_64.AppImage
./EzooDownload-*-x86_64.AppImage
```

The `.deb`, `.rpm`, and AppImage packages include ffmpeg, so merging video and audio works without installing anything else. A Flatpak bundle is not attached to the 1.1.0 release yet — it is still under review and will be added in a later release.

For troubleshooting, see the [Linux guide](packaging/linux/README-Linux.md).

</details>

<details>
<summary><b>Browser extension</b></summary>

Download the Chrome or Firefox package from the [latest release](https://github.com/kerlif/ezoo-download/releases/latest).

- **Chrome:** extract the zip, open `chrome://extensions`, turn on Developer mode, and choose **Load unpacked**.
- **Firefox:** open the `.xpi` file. Firefox may ask you to confirm the install, and it may refuse add-ons that are not signed.

</details>

<details>
<summary><b>Build from source</b></summary>

You need the stable Rust toolchain and the build tools for your platform.

```bash
git clone https://github.com/kerlif/ezoo-download.git
cd ezoo-download
cargo build --release
```

To build the Windows installer, run `build-windows.ps1`. For Linux packages, see the [Linux guide](packaging/linux/README-Linux.md).

</details>

## Privacy and network

- Downloads go directly from the server you chose to your computer.
- Two automatic network requests: an update check (app name and version only, every 24 hours — you can turn it off in settings), and a tool update check that fetches yt-dlp and aria2 at most once a day and verifies their checksums before use.
- yt-dlp runs only when you request video, playlist, or tool-update features.
- No telemetry, analytics, or automatic crash reports.

## Responsible use

> [!IMPORTANT]
> Ezoo Download refuses DRM-protected streams and does not try to bypass DRM. Download only content you have the right to download, and follow the terms of each website you use.

## FAQ

<details>
<summary><b>Is it free?</b></summary>

Yes. There are no ads, no subscriptions, and no locked features.

</details>

<details>
<summary><b>Does it support YouTube, Netflix, or other sites?</b></summary>

The app does not promise support for any specific site. Streams protected by DRM are refused, and much of the content on paid streaming services uses DRM. Downloading from any site may breach that site's terms of service, so check them first.

</details>

<details>
<summary><b>Why doesn't choosing a quality change the download?</b></summary>

In this version the quality list is for information only, and the download uses the default quality. Choosing a quality that changes the download may be added in a future release.

</details>

<details>
<summary><b>Does it run on Linux?</b></summary>

Yes, with `.deb`, `.rpm`, and AppImage packages. A Flatpak bundle is planned for a later release.

</details>

<details>
<summary><b>What happens if the connection drops?</b></summary>

The app tries to resume the download automatically. You can also resume it yourself at any time.

</details>

<details>
<summary><b>Is the source code available?</b></summary>

Yes, in this repository. The original project was written by Ahmed Ezzat (Ezoo Tech).

</details>

## Contributing

Open an issue to report a bug or suggest a feature. Translation fixes are welcome too.

## License

- The original project is by **Ahmed Ezzat (Ezoo Tech)**.
- Linux packaging and development in this repository are by **kerlif**.
- The project license is in this repository's license files. Third-party components are listed in `THIRD-PARTY-NOTICES.txt`.

<div align="center">

<br>

<img src="assets/logo.svg" width="56" alt="Ezoo Download logo">

<br>

<sub>Built with Rust.</sub>

</div>
