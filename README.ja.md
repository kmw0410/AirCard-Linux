# AirCard (Linux) 🎴

[Korean](README.ko.md) | [English](README.md) | [Japanese](README.ja.md)

> **脱獄不要の Linux 向け Apple Wallet カード画像変更・ロック画面パスコードテーマアプリ**
> Wallet カードの画像変更機能のみ、iOS 27.2 の iPhone でテスト済みです。パスコードテーマは実機で未検証であり、ほかの iOS バージョンも未テストです。
> `airlift` の AirTraffic 同期エクスプロイトを使用します。

AirCard (Linux) は [AirCard](https://github.com/Mak5er/AirCard) を GTK4/libadwaita に移植したものです。

---

## 機能

- 🎨 **カード画像の変更:** Apple Wallet の各カードに画像を適用します。画像は中央を基準に切り抜き、1536 × 969 にリサイズされます。
- 🔢 **ロック画面のパスコードテーマ (.passthm):** 既存の `.passthm` ファイルからキーパッド画像を適用します。Linux 側の実装はありますが、実機では未検証です。テスト済みなのは Wallet の画像変更のみです。
- 📱 **カード検出:** デバイスログのスキャン中に Wallet でカードを開くと、そのカードのハッシュを自動入力します。
- 🐧 **GTK AppImage:** x86_64 Linux 向けの GTK4/libadwaita インターフェースです。AppImage にはアプリと `libimobiledevice` ユーティリティが含まれ、ホスト側には `usbmuxd` が必要です。
- 🌐 **多言語対応**
- 💾 **Books 同期ファイルのバックアップ:** 適用操作の前に影響を受ける Books 同期ファイルを保存し、操作後に復元します。

---

## インストール

### Linux (x86_64 AppImage)

1. ホストに `usbmuxd` をインストールして起動します。Arch Linux/CachyOS では `sudo pacman -S usbmuxd` を実行します。
2. Linux ビルドから `AirCard-x86_64.AppImage` を入手するか、[ソースからビルド](#ソースからビルド)します。
3. `chmod +x AirCard-x86_64.AppImage` で実行権限を付け、`./AirCard-x86_64.AppImage` を起動します。
4. データ転送対応の USB ケーブルで iPhone を接続し、ロックを解除して **このコンピュータを信頼** を承認します。

AppImage は CachyOS x86_64 で起動を確認しました。ほかのディストリビューションとの互換性は未検証です。脱獄は不要です。

---

## Apple Wallet カード画像の変更方法

1. iPhone を接続して信頼を承認し、**Device** で **Refresh devices** をクリックします。
2. **Walletカード** で **Walletログをスキャン** をクリックし、60 秒以内に Wallet で変更したいカードを開きます。検出したハッシュはカードのハッシュ入力欄に自動入力されます。既知のハッシュを貼り付けることもできます。
3. **画像を選択** をクリックし、PNG、JPEG、または WebP 画像を選びます。
4. **カード画像を適用** をクリックし、**アクティビティ** で結果を確認します。
5. iPhone の **Wallet** を完全に終了してからカードを開き直し、新しい画像を確認します。

Wallet の画像変更は iOS 27.2 の iPhone で確認済みです。パスコードテーマは未テストです。`Artwork sent; 0 cache entries removed` のようなメッセージは、転送は完了したものの一致するキャッシュ項目は削除されなかったことを意味します。Wallet で実際の表示を確認してください。

### スキャンでカードが見つからない場合

iPhone のロックを解除し、**Device** で対象デバイスが選択されていることを確認して、再度スキャンしてください。スキャン中に Wallet を開き、対象カードをタップするかカードを切り替えてください。ハッシュが検出されなければ、iPhone を再接続して試してください。iOS のログによってはカード識別子が省略・マスクされるため、すべてのバージョンでの検出は保証されません。

接続の診断には `idevice_id -l` または `./AirCard-x86_64.AppImage --list-devices` を実行します。`./AirCard-x86_64.AppImage --probe-device <UDID>` はデバイス上のファイルを変更せず、ペアリングと AFC アクセスを確認します。デバイスが表示されない場合は `systemctl status usbmuxd.service` を確認し、iPhone のロックを解除して `idevicepair pair` を実行してください。

---

## ロック画面のパスコードテーマ (.passthm) の適用方法

1. iPhone を接続し、デバイス欄で選択します。
2. **パスコードテーマ** を展開し、**.passthmテーマを選択** をクリックします。
3. 既存の `.passthm` ファイルを選び、**パスコードテーマを適用** をクリックします。
4. iPhone を再起動してロック画面のキャッシュを再読み込みし、結果を確認します。

> [!NOTE]
> パスコードテーマの適用は実機で未検証です。テスト済みなのは Wallet の画像変更のみです。アプリはテーマのキーパッド画像を、英語およびほかのロケールのファイル名バリエーションの通常表示・太字表示アセットへ展開しますが、ロケールごとの表示結果は未テストです。

適用操作の前に、AirCard はデバイスの Books 同期ファイルを `~/.local/share/aircard/backups/<UDID>/` (または `$XDG_DATA_HOME/aircard/backups/`) に保存します。自動復元が完了しなかった場合は iPhone を再接続し、`./AirCard-x86_64.AppImage --restore-books <UDID> <backup-directory>` を実行してください。

---

## ソースからビルド

Rust、GTK4/libadwaita 開発パッケージ、`libimobiledevice` (`idevice_id`、`ideviceinfo`、`idevicesyslog` を含む)、`usbmuxd` をインストールします。GTK アプリをビルドして実行するには:

```sh
cargo run --release
```

AppImage をビルドするには `linuxdeploy` と `appimagetool` をインストールし、次を実行します:

```sh
./packaging/build-appimage.sh
```

出力ファイルは `dist/AirCard-x86_64.AppImage` です。

---

## 貢献者

- **[@mak5er](https://github.com/mak5er)** (オリジナル AirCard 開発者) — [GitHub](https://github.com/mak5er) · [Twitter / X](https://x.com/mak5er)
- **[@Lumid-Off](https://github.com/Lumid-Off)** (AirCard の貢献者・開発者) — [GitHub](https://github.com/Lumid-Off) · [Twitter / X](https://x.com/LumidOff)
- **[AirLift](https://github.com/0xjohnnydev/airlift)**、**[0xjohnny (@0xjohnnydev)](https://github.com/0xjohnnydev)** 作: オリジナルの AirTraffic/ATAirlock サンドボックス脱出と概念実証。

## クレジット

- Linux の AirTraffic と StreamingZip の実装は、MIT ライセンスの [AirCard-iOS](https://github.com/Mak5er/AirCard-iOS) および [AirCard-Windows](https://github.com/Lumid-Off/AirCard-Windows) をもとに調整しました。
- 中核のエクスプロイトは [`airlift`](https://github.com/0xjohnnydev/airlift) (AirTraffic 同期エスケープ) に基づきます。
