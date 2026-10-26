# Night Freeway SS

[日本語](#日本語) / [English](#english)

## 日本語

夜の高速道路を光の点で描く、Windows用のスクリーンセイバーです。mhatta氏の[nightfreeway](https://github.com/mhatta/nightfreeway)を参考に、独自に実装しました。

### 動作環境

- Windows 10 / 11
- x64版(一般的なIntel/AMDのPC)と、ARM64版(ARMベースのプロセッサのPC)があります

### 同梱ファイル

| ファイル | 内容 |
| --- | --- |
| `NightFreewaySS.scr` | スクリーンセイバー本体 |
| `LICENSE` | ライセンス(MIT) |
| `THIRD-PARTY-NOTICES.txt` | 本体に組み込まれている第三者のソフトウェアの著作権表示とライセンス |
| `README.md` | この説明書 |

### インストール

1. zipを、スクリーンセイバーを置いておくフォルダーに展開します。
2. 展開した`NightFreewaySS.scr`を右クリックし、「インストール」を選びます。スクリーンセイバーの設定画面が開き、一覧に「Night Freeway SS」が加わります。

Windowsの「インストール」はファイルをコピーせず、展開した場所の`NightFreewaySS.scr`をそのまま登録します。このファイルを移動・削除すると、スクリーンセイバーは動かなくなります。

デジタル署名がないため、Microsoft Defender SmartScreenの警告が表示されることがあります。警告の「詳細情報」から実行できます。

### 設定

スクリーンセイバーの設定画面で「Night Freeway SS」を選び、「設定」ボタンを押すと、次の項目を変えられます。

| 項目 | 設定画面の表示 | 範囲 | 既定値 |
| --- | --- | --- | --- |
| 巡航速度 | Cruising speed | 30〜160 km/h(10 km/h刻み) | 100 km/h |
| 通行方向 | Driving side | 左側通行 / 右側通行 | 左側通行 |
| 速度計の表示 | Show speedometer | オン / オフ | オフ |
| UFOの表示 | Show a UFO now and then | オン / オフ(オンのとき、平均10分に1回、UFOが現れる) | オフ |

設定は、レジストリの`HKEY_CURRENT_USER\Software\NightFreewaySS`に保存されます。

### アンインストール

1. スクリーンセイバーの設定画面で、別のスクリーンセイバーまたは「(なし)」を選びます。
2. 展開したフォルダーごと削除します。
3. 保存された設定も消す場合は、コマンドプロンプトで次のコマンドを実行します。

   ```text
   reg delete HKCU\Software\NightFreewaySS /f
   ```

### ライセンスと謝辞

[MITライセンス](LICENSE)で配布しています。ソースコードは<https://github.com/suchi/freeway-ss>にあります。

本ソフトウェアは、mhatta氏による[nightfreeway](https://github.com/mhatta/nightfreeway)(Copyright (c) 2026 Masayuki Hatta、MITライセンス)の挙動と見た目を参考にしています。コードは移植せず、独自に実装しています。nightfreewayの謝辞にならい、原作のFreeWayの作者であるFKS氏、Windows移植を手がけたYoshihiro Sugahara氏、nightfreewayが参考にしたsoyokaze2000.comのFreeWay再現版にも感謝します。

### 免責事項

本ソフトウェアは、AIコーディングエージェントにコードの作成の大部分を任せる、いわゆるVibe Codingで開発しました。作者は設計の判断と実機での動作確認を行っていますが、すべてのコードを詳細に検証しているわけではありません。

そのため、作者は本ソフトウェアの品質・安全性・動作を保証せず、本ソフトウェアの利用によって生じた不具合・損害・トラブルについて責任を負いません。これは[MITライセンス](LICENSE)の無保証の条項を、改めて明示するものです。

---

## English

Night Freeway SS is a screensaver for Windows that draws a freeway at night with points of light. It is an independent implementation based on [nightfreeway](https://github.com/mhatta/nightfreeway) by mhatta.

### Requirements

- Windows 10 or 11
- There are two versions: x64 (for most Intel and AMD PCs) and ARM64 (for PCs with an ARM-based processor).

### Files

| File | Description |
| --- | --- |
| `NightFreewaySS.scr` | The screensaver |
| `LICENSE` | License (MIT) |
| `THIRD-PARTY-NOTICES.txt` | Copyright notices and licenses of the third-party software included in the screensaver |
| `README.md` | This document |

### Installation

1. Extract the zip file to the folder where you keep the screensaver.
2. Right-click the extracted `NightFreewaySS.scr` and select "Install". The screensaver settings open, and "Night Freeway SS" is added to the list.

"Install" in Windows does not copy the file. It registers `NightFreewaySS.scr` in the folder where it was extracted. If the file is moved or deleted, the screensaver stops working.

Because the file has no digital signature, Microsoft Defender SmartScreen may show a warning. The program can be run from "More info" in the warning.

### Settings

Select "Night Freeway SS" in the screensaver settings and click "Settings" to change the following items.

| Item | Range | Default |
| --- | --- | --- |
| Cruising speed | 30 to 160 km/h (in steps of 10 km/h) | 100 km/h |
| Driving side | Left-hand traffic / Right-hand traffic | Left-hand traffic |
| Show speedometer | On / Off | Off |
| Show a UFO now and then | On / Off (when on, a UFO appears about once every 10 minutes on average) | Off |

The settings are saved in the registry under `HKEY_CURRENT_USER\Software\NightFreewaySS`.

### Uninstallation

1. In the screensaver settings, select another screensaver or "(None)".
2. Delete the folder where the zip file was extracted.
3. To also remove the saved settings, run the following command in Command Prompt.

   ```text
   reg delete HKCU\Software\NightFreewaySS /f
   ```

### License and acknowledgments

This software is distributed under the [MIT License](LICENSE). The source code is available at <https://github.com/suchi/freeway-ss>.

The behavior and appearance of this software are based on [nightfreeway](https://github.com/mhatta/nightfreeway) by mhatta (Copyright (c) 2026 Masayuki Hatta, MIT License). No code was ported; this software is an independent implementation. Following the acknowledgments of nightfreeway, we also thank FKS, the author of the original FreeWay; Yoshihiro Sugahara, who ported it to Windows; and the FreeWay reproduction on soyokaze2000.com, which nightfreeway referred to.

### Disclaimer

This software was developed by so-called vibe coding, in which most of the code is written by AI coding agents. The author made the design decisions and tested the software on real machines, but has not reviewed all of the code in detail.

Therefore, the author does not guarantee the quality, safety, or operation of this software, and is not responsible for any defects, damage, or trouble caused by its use. This restates the warranty disclaimer of the [MIT License](LICENSE).
