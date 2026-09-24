# AirCard (Linux) 🎴

[Korean](README.ko.md) | [English](README.md) | [Japanese](README.ja.md)

> **탈옥 없이 Linux에서 Apple Wallet 카드 이미지를 변경하고 잠금 화면 암호 테마를 적용하는 앱**
> Wallet 카드 이미지 변경 기능만 iOS 27.2 iPhone에서 테스트했습니다. 암호 테마는 아직 실기기에서 검증하지 않았고, 다른 iOS 버전도 테스트하지 않았습니다.
> `airlift` AirTraffic 동기화 익스플로잇을 사용합니다.

AirCard (Linux)는 [AirCard](https://github.com/Mak5er/AirCard)의 GTK4/libadwaita 포팅 버전입니다.

---

## 기능

- 🎨 **카드 이미지 변경:** Apple Wallet의 개별 카드에 이미지를 적용합니다. 이미지는 중앙을 기준으로 자른 뒤 1536 × 969 크기로 조정됩니다.
- 🔢 **잠금 화면 암호 테마(.passthm):** 기존 `.passthm` 파일의 키패드 이미지를 적용합니다. Linux 구현은 완료했지만 실기기에서는 아직 검증하지 않았습니다. 테스트된 기능은 Wallet 이미지 변경뿐입니다.
- 📱 **카드 감지:** 기기 로그를 스캔하는 동안 Wallet에서 카드를 열면 카드 해시를 자동으로 입력합니다.
- 🐧 **GTK AppImage:** x86_64 Linux용 GTK4/libadwaita 인터페이스입니다. AppImage에 앱과 `libimobiledevice` 유틸리티가 포함되어 있으며, 호스트에는 `usbmuxd`가 필요합니다.
- 🌐 **다중언어 지원**
- 💾 **Books 동기화 백업:** 적용 작업 전에 영향을 받는 Books 동기화 파일을 백업하고 작업 후 복원합니다.

---

## 설치

### Linux (x86_64 AppImage)

1. 호스트에 `usbmuxd`를 설치하고 실행합니다. Arch Linux/CachyOS에서는 `sudo pacman -S usbmuxd`를 실행합니다.
2. Linux 빌드에서 `AirCard-x86_64.AppImage`를 받거나 [소스에서 빌드](#소스에서-빌드)합니다.
3. `chmod +x AirCard-x86_64.AppImage`로 실행 권한을 부여한 다음 `./AirCard-x86_64.AppImage`를 실행합니다.
4. 데이터 전송이 가능한 USB 케이블로 iPhone을 연결하고 잠금을 해제한 뒤 **이 컴퓨터 신뢰**를 승인합니다.

AppImage는 CachyOS x86_64에서 실행해 보았습니다. 다른 배포판과의 호환성은 아직 검증하지 않았습니다. 탈옥은 필요하지 않습니다.

---

## Apple Wallet 카드 이미지 변경

1. iPhone을 연결하고 신뢰를 승인한 뒤 **Device**에서 **Refresh devices**를 누릅니다.
2. **Activity**에서 **Scan Wallet logs**를 누르고, 60초 안에 Wallet에서 변경할 카드를 엽니다. 감지된 해시는 **Wallet Cards** 탭에 자동 입력됩니다. 이미 알고 있는 해시를 직접 붙여 넣어도 됩니다.
3. **Wallet Cards**에서 **Choose skin image**를 누르고 PNG, JPEG 또는 WebP 이미지를 선택합니다.
4. **Apply card skin**을 누른 뒤 **Activity**에서 결과를 확인합니다.
5. iPhone에서 **Wallet**을 완전히 종료한 다음 카드를 다시 열어 변경된 이미지를 확인합니다.

Wallet 이미지 변경은 iOS 27.2 iPhone에서 확인했습니다. 암호 테마는 테스트하지 않았습니다. `Artwork sent; 0 cache entries removed` 같은 메시지는 전송은 완료했지만 일치하는 캐시 항목은 제거하지 못했다는 뜻이므로, Wallet에서 실제 결과를 확인하세요.

### 스캔에서 카드가 감지되지 않는 경우

iPhone 잠금을 해제하고 **Device**에서 해당 기기가 선택되어 있는지 확인한 다음 다시 스캔하세요. 스캔 중 Wallet을 열고 대상 카드를 누르거나 카드 사이를 전환해 보세요. 해시가 감지되지 않으면 iPhone을 다시 연결한 뒤 재시도하세요. 일부 iOS 로그에서는 카드 식별자가 생략되거나 가려질 수 있어 모든 버전에서 감지를 보장할 수 없습니다.

연결 상태를 확인하려면 `idevice_id -l` 또는 `./AirCard-x86_64.AppImage --list-devices`를 실행하세요. `./AirCard-x86_64.AppImage --probe-device <UDID>`는 기기 파일을 수정하지 않고 페어링과 AFC 접근을 확인합니다. 기기가 표시되지 않으면 `systemctl status usbmuxd.service`를 확인하고, iPhone 잠금을 해제한 상태에서 `idevicepair pair`를 실행하세요.

---

## 잠금 화면 암호 테마(.passthm) 적용

1. iPhone을 연결하고 **Device**에서 선택합니다.
2. **Passcode Themes**에서 **Choose .passthm theme**을 누릅니다.
3. 기존 `.passthm` 파일을 선택하고 **Apply passcode theme**을 누릅니다.
4. iPhone을 재시작해 잠금 화면 캐시를 다시 불러온 뒤 결과를 확인합니다.

> [!NOTE]
> 암호 테마 적용은 아직 실기기에서 검증하지 않았습니다. 테스트한 기능은 Wallet 이미지 변경뿐입니다. 앱은 테마의 키패드 이미지를 영어 및 다른 로케일용 파일 이름 변형의 일반 글꼴·볼드체 자산으로 확장하지만, 로케일별 표시 결과는 테스트하지 않았습니다.

적용 작업 전에 AirCard는 기기의 Books 동기화 파일을 `~/.local/share/aircard/backups/<UDID>/`(또는 `$XDG_DATA_HOME/aircard/backups/`)에 저장합니다. 자동 복원이 완료되지 않으면 iPhone을 다시 연결하고 `./AirCard-x86_64.AppImage --restore-books <UDID> <backup-directory>`를 실행하세요.

---

## 소스에서 빌드

Rust, GTK4/libadwaita 개발 패키지, `libimobiledevice`(`idevice_id`, `ideviceinfo`, `idevicesyslog` 포함), `usbmuxd`를 설치합니다. GTK 앱을 빌드하고 실행하려면:

```sh
cargo run --release
```

AppImage를 빌드하려면 `linuxdeploy`와 `appimagetool`을 설치한 다음 실행합니다:

```sh
./packaging/build-appimage.sh
```

출력 파일은 `dist/AirCard-x86_64.AppImage`입니다.

---

## 기여자

- **[@mak5er](https://github.com/mak5er)** (원본 AirCard 개발자) — [GitHub](https://github.com/mak5er) · [Twitter / X](https://x.com/mak5er)
- **[@Lumid-Off](https://github.com/Lumid-Off)** (AirCard 기여자 및 개발자) — [GitHub](https://github.com/Lumid-Off) · [Twitter / X](https://x.com/LumidOff)
- **[AirLift](https://github.com/0xjohnnydev/airlift)**, **[0xjohnny (@0xjohnnydev)](https://github.com/0xjohnnydev)** 제작: 원본 AirTraffic/ATAirlock 샌드박스 탈출 및 개념 증명.

## 크레딧

- Linux AirTraffic 및 StreamingZip 구현은 MIT 라이선스의 [AirCard-iOS](https://github.com/Mak5er/AirCard-iOS)와 [AirCard-Windows](https://github.com/Lumid-Off/AirCard-Windows) 프로젝트를 바탕으로 조정했습니다.
- 핵심 익스플로잇은 [`airlift`](https://github.com/0xjohnnydev/airlift)(AirTraffic 동기화 탈출)를 기반으로 합니다.
