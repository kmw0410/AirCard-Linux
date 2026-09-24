# AirCard (Linux) 🎴

> **Apple Wallet Card Skinner & Lockscreen Passcode Themer for Linux (No Jailbreak Required)**
> Only the Wallet card image-changing feature has been tested on an iPhone running iOS 27.2. Passcode themes have not yet been verified on a device. Other iOS versions are also untested.
> Powered by the `airlift` AirTraffic sync exploit.

AirCard (Linux) is a GTK4/libadwaita port of [AirCard](https://github.com/Mak5er/AirCard).

---

## Features

- 🎨 **Custom Card Skins:** Assign an image to an individual Apple Wallet card. Images are center-cropped and resized to 1536 × 969.
- 🔢 **Lock Screen Passcode Themes (.passthm):** Apply keypad artwork from an existing `.passthm` file. This Linux path is implemented but has not yet been verified; only Wallet image changing has been tested.
- 📱 **Card Detection:** Open a card in Wallet while scanning device logs to fill its card hash automatically.
- 🐧 **GTK AppImage:** GTK4/libadwaita interface for x86_64 Linux. The AppImage bundles the application and its `libimobiledevice` utilities; the host must provide `usbmuxd`.
- 🌐 **다중언어 지원**
- 💾 **Books Sync Backup:** Save the affected Books sync files before each apply operation and restore them afterward.

---

## Installation

### Linux (x86_64 AppImage)

1. Install and run `usbmuxd` on the host. On Arch Linux/CachyOS: `sudo pacman -S usbmuxd`.
2. Get `AirCard-x86_64.AppImage` from a Linux build or [build it from source](#building-from-source).
3. Make it executable with `chmod +x AirCard-x86_64.AppImage`, then run `./AirCard-x86_64.AppImage`.
4. Connect your iPhone via a data-capable USB cable, unlock it, and approve **Trust This Computer**.

The AppImage has been run on CachyOS x86_64; compatibility with other distributions has not yet been verified. No jailbreak is required.

---

## How to Customize Apple Wallet Cards

1. Connect and trust your iPhone, then open **Device** and click **Refresh devices**.
2. Open **Activity**, click **Scan Wallet logs**, and open the card you want to customize in Wallet within 60 seconds. The detected hash is filled into the **Wallet Cards** tab. You can also paste a known hash there.
3. In **Wallet Cards**, click **Choose skin image** and select a PNG, JPEG, or WebP image.
4. Click **Apply card skin** and wait for the result in **Activity**.
5. Force-close **Wallet** on the iPhone and reopen the card to inspect the new artwork.

The Wallet image-changing flow was confirmed on an iPhone running iOS 27.2; passcode themes were not tested. A message such as `Artwork sent; 0 cache entries removed` means the transfer finished but no matching cache entries were removed; check the card in Wallet to confirm the visible result.

### If scanning finds no cards

Unlock the iPhone, confirm it is still selected under **Device**, and start another scan. While scanning, open Wallet and tap or switch to the target card. If the scan ends without a hash, reconnect the phone and retry. Some iOS log messages may omit or redact card identifiers, so detection is not guaranteed on every version.

For connection diagnostics, run `idevice_id -l` or `./AirCard-x86_64.AppImage --list-devices`. `./AirCard-x86_64.AppImage --probe-device <UDID>` checks pairing and AFC access without modifying device files. If no device appears, check `systemctl status usbmuxd.service` and run `idevicepair pair` with the phone unlocked.

---

## How to Apply Lockscreen Passcode Themes (.passthm)

1. Connect and select your iPhone under **Device**.
2. Open **Passcode Themes** and click **Choose .passthm theme**.
3. Select an existing `.passthm` file and click **Apply passcode theme**.
4. Restart the iPhone to reload the lock-screen cache and inspect the result.

> [!NOTE]
> Passcode theme application has not yet been verified on a device. Only Wallet image changing has been tested. The app expands the theme's keypad images into standard and Bold Text assets for English and other locale filename variants, but appearance across locales is untested.

Before an apply operation, AirCard saves the device's Books sync files under `~/.local/share/aircard/backups/<UDID>/` (or `$XDG_DATA_HOME/aircard/backups/`). If automatic restoration cannot complete, reconnect the phone and run `./AirCard-x86_64.AppImage --restore-books <UDID> <backup-directory>`.

---

## Building from Source

Install Rust, GTK4/libadwaita development packages, `libimobiledevice` (including `idevice_id`, `ideviceinfo`, and `idevicesyslog`), and `usbmuxd`. To build and run the GTK app:

```sh
cargo run --release
```

To build the AppImage, install `linuxdeploy` and `appimagetool`, then run:

```sh
./packaging/build-appimage.sh
```

The output is `dist/AirCard-x86_64.AppImage`.

---

## Contributors

- **[@mak5er](https://github.com/mak5er)** (Original AirCard developer) — [GitHub](https://github.com/mak5er) · [Twitter / X](https://x.com/mak5er)
- **[@Lumid-Off](https://github.com/Lumid-Off)** (AirCard contributor and developer) — [GitHub](https://github.com/Lumid-Off) · [Twitter / X](https://x.com/LumidOff)
- **[AirLift](https://github.com/0xjohnnydev/airlift)** by **[0xjohnny (@0xjohnnydev)](https://github.com/0xjohnnydev)**: Original AirTraffic/ATAirlock sandbox escape and proof of concept.

## Credits

- The Linux AirTraffic and StreamingZip implementation was adapted from the MIT-licensed [AirCard-iOS](https://github.com/Mak5er/AirCard-iOS) and [AirCard-Windows](https://github.com/Lumid-Off/AirCard-Windows) projects.
- Core exploit based on [`airlift`](https://github.com/0xjohnnydev/airlift) (AirTraffic sync escape).
