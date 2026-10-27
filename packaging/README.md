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
| 山並みの表示 | Show distant mountains | オン / オフ(オンのとき、地平線のところどころの方向に山並みが見え、山頂の赤い灯火がゆっくり点滅する) | オン |
| 巡航速度のゆらぎ | Vary the cruising speed | オン / オフ(オンのとき、設定した速度の±20 km/hの範囲で、数分かけてゆっくり変わる。速度計は実際の速さを表示する) | オン |
| 時刻に応じた交通量 | Traffic follows the time of day | オン / オフ(オンのとき、起動時のPCの時刻に合わせて、夜中と早朝は他の車が少なくトラックが多く、日中は従来どおりになる) | オン |
| 景色の区間 | Alternate mountains and plains | オン / オフ(オンのとき、100 km/hで10〜20分ごとに、山並みとトンネルのある山あいと、山並みもトンネルもない平野が入れ替わる) | オン |
| トラック | Trucks | なし / 日本風 / 北米風(他の車の約5台に1台がトラックになり、日本風は上側の端灯と側面の橙のサイドマーカー、北米風は後ろの識別灯と車幅灯が付く) | 日本風 |
| トンネル | Tunnels | なし / 混在 / ナトリウム灯(橙) / LED(白)(なし以外のとき、100 km/hで平均3分に1本、通り抜けるのに20秒〜1分のトンネルが現れる。混在はトンネルごとに照明の色が変わる。トンネルの中では空が見えない) | 混在 |
| 他車の車体 | Car bodies | 透明(灯火だけ) / 半透明の黒 / 黒(透明のときは車の灯火だけを描く。半透明の黒と黒のときは、先行車・追い越し車・対向車(トラックを含む)の車体を黒っぽい箱(端・側面・屋根の面ごとに濃さが違う)で描き、車の奥の光を隠す。半透明の黒は奥の光が1割だけ透け、黒は完全に隠す) | 透明 |
| 表示言語 | Language | 自動(Windowsの表示言語) / English / 日本語 | 自動 |

表示言語は、設定画面とエラーメッセージの言語です。「自動」のときは、Windowsの表示言語が日本語なら日本語、それ以外は英語で表示します。設定画面で言語を選ぶと、その場で表示が切り替わります(保存するには「OK」を押します)。

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
| Show distant mountains | On / Off (when on, mountain ranges stand on the horizon in some directions, and red lights on some peaks blink slowly) | On |
| Vary the cruising speed | On / Off (when on, the speed slowly changes over a few minutes within 20 km/h of the chosen speed; the speedometer shows the actual speed) | On |
| Traffic follows the time of day | On / Off (when on, the traffic follows the PC's time at startup: fewer cars and more trucks late at night and early in the morning, and as usual during the day) | On |
| Alternate mountains and plains | On / Off (when on, every 10 to 20 minutes at 100 km/h, mountain areas with mountain ranges and tunnels and plains with neither take turns) | On |
| Trucks | None / Japanese style / North American style (about one in five other vehicles is a truck; Japanese style has upper marker lamps and amber side markers, North American style has identification and clearance lamps) | Japanese style |
| Tunnels | None / Mixed lighting / Sodium (orange) / LED (white) (unless None, a tunnel that takes 20 to 60 seconds to drive through appears about once every 3 minutes at 100 km/h; Mixed lighting changes the lamp color from tunnel to tunnel; the sky is not seen inside a tunnel) | Mixed lighting |
| Car bodies | Transparent (lights only) / Translucent black / Black (when transparent, only the lights of the cars are drawn; with translucent black or black, the bodies of the cars ahead, the passing cars and the oncoming cars, trucks included, are drawn as blackish boxes, with a different shade for the end, the sides and the roof, that hide the lights behind them; translucent black lets a tenth of the light through, and black hides it completely) | Transparent |
| Language | Automatic (Windows display language) / English / 日本語 | Automatic |

The language is used for the settings dialog and error messages. With "Automatic", Japanese is shown when the Windows display language is Japanese, and English otherwise. When you choose a language in the dialog, the display changes at once (click "OK" to save it).

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
