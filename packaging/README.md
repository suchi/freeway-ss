# Night Freeway SS

[日本語](#日本語) / [English](#english)

## 日本語

夜の高速道路を光の点で描く、Windows用のスクリーンセーバーです。mhatta氏の[nightfreeway](https://github.com/mhatta/nightfreeway)を参考に、独自に実装しました。

### 動作環境

- Windows 10 / 11
- x64版(一般的なIntel/AMDのPC)と、ARM64版(ARMベースのプロセッサのPC)があります

### 同梱ファイル

| ファイル | 内容 |
| --- | --- |
| `NightFreewaySS.scr` | スクリーンセーバー本体 |
| `LICENSE` | ライセンス(MIT) |
| `THIRD-PARTY-NOTICES.txt` | 本体に組み込まれている第三者のソフトウェアの著作権表示とライセンス |
| `README.md` | この説明書 |

### インストール

1. zipを、スクリーンセーバーを置いておくフォルダーに展開します。
2. 展開した`NightFreewaySS.scr`を右クリックし、「インストール」を選びます。スクリーンセーバーの設定画面が開き、一覧に「Night Freeway SS」が加わります。

Windowsの「インストール」はファイルをコピーせず、展開した場所の`NightFreewaySS.scr`をそのまま登録します。このファイルを移動・削除すると、スクリーンセーバーは動かなくなります。

デジタル署名がないため、Microsoft Defender SmartScreenの警告が表示されることがあります。警告の「詳細情報」から実行できます。

### 設定

スクリーンセーバーの設定画面で「Night Freeway SS」を選び、「設定」ボタンを押すと、次の項目を変えられます。

| 項目 | 設定画面の表示 | 範囲 | 既定値 |
| --- | --- | --- | --- |
| 巡航速度 | Cruising speed | 30〜160 km/h(10 km/h刻み) | 100 km/h |
| 通行方向 | Driving side | 左側通行 / 右側通行 | 左側通行 |
| 速度計 | Speedometer | なし / 数字だけ / ダッシュボード(シンプル) / ダッシュボード(作り込み)(数字だけは左下に速さの数字を表示する。ダッシュボード(シンプル)は円弧のゲージと数字、ダッシュボード(作り込み)は目盛りと針のある速度計・回転計・時刻・走った距離・シフトの位置を表示する。前の版で速度計を表示する設定にしていた場合は、数字だけになる) | なし |
| 山並みの表示 | Show distant mountains | オン / オフ(オンのとき、地平線のところどころの方向に山並みが見え、山頂の赤い灯火がゆっくり点滅する) | オン |
| 巡航速度のゆらぎ | Vary the cruising speed | オン / オフ(オンのとき、設定した速度の±20 km/hの範囲で、1〜2分ほどかけてゆっくり変わる。速度計は実際の速さを表示する) | オン |
| 時刻に応じた交通量 | Traffic follows the time of day | オン / オフ(オンのとき、起動時のPCの時刻に合わせて、夜中と早朝は他の車が少なくトラックが多く、日中は従来どおりになる) | オン |
| 景色の区間 | Alternate mountains and plains | オン / オフ(オンのとき、100 km/hで10〜20分ごとに、山並みとトンネルのある山あいと、山並みもトンネルもない平野が入れ替わる) | オン |
| トラック | Trucks | なし / 日本風 / 北米風(他の車の約5台に1台がトラックになり、日本風は大型の箱車とトラクターとトレーラー(上側の端灯と側面の橙のサイドマーカーが付く)、北米風はボンネット型のセミトレーラーと箱型トラック(識別灯・車幅灯と、前寄りが橙で後ろ寄りが赤の側面のマーカーが付く)になる。他車の車体が「ブロック」のときは、タイヤ・運転台・荷台のある箱の組み合わせで、「詳細な形」のときは、運転台の前面の傾きや屋根の風よけのある形で、「シンプルな箱」のときは全体を囲む1つの箱で描く) | 日本風 |
| トンネル | Tunnels | なし / 混在 / ナトリウム灯(橙) / LED(白)(なし以外のとき、100 km/hで平均3分に1本、通り抜けるのに20秒〜1分のトンネルが現れる。混在はトンネルごとに照明の色が変わる。トンネルの中では空が見えない) | 混在 |
| 他車の車体 | Car bodies | なし(灯火だけ) / シンプルな箱 / ブロック / 詳細な形(なしのときは車の灯火だけを描く。シンプルな箱・ブロック・詳細な形のときは、先行車・追い越し車・対向車(トラックを含む)の車体を黒っぽい面(端・側面・屋根の面ごとに濃さが違う)で描き、車の奥の光を隠す。シンプルな箱は、どの車も全体を囲む1つの箱で描く。ブロックは、普通車をセダン・ハッチバック・ミニバン・SUVの形で、箱の組み合わせで描き(トラックの設定が日本風のときは軽自動車、北米風のときはピックアップトラックも加わる)、トラックを運転台・荷台・タイヤのある形で描く。詳細な形は、同じ車種を、ボンネット・傾いたフロントガラスと窓(車体より少し青みのある暗い色)・屋根に向かって狭まる側面のある実際の車に近い輪郭で描き、トラックの運転台も輪郭のある形で描く(遠くの車はブロックで描く)。車体を描くときは、中央分離帯の街灯の路面の光と、トンネルの中の路面の光の上に、他車の影が落ち、影はその光だけをさえぎり、縁がやわらかい。前の版で車体を描く設定にしていた場合は、ブロックになる) | なし |
| 車体の色 | Body color | 半透明の黒 / 黒(車体を描くときの色。半透明の黒は奥の光が1割だけ透け、黒は完全に隠す。車体がなしのときは使われず、設定画面では選べない) | 半透明の黒 |
| 車線変更 | Lane changes | なし / 自車だけ / 自車と他車(自車だけのとき、自分の車が遅い前の車に追いつくと、追い越し車線が空いていれば、ウインカーを3秒点滅させてから追い越し車線に移って抜き、元の車線に戻る。空いていなければ、前の車との間を保って後ろを走り、空いたら抜く。自車と他車のときは、他の車も同じように車線を変える。車線を変える車は、移る側の後ろの角で橙のウインカーを点滅させる。自分の車が車線を移るあいだは、視点がわずかに進む方向へ振れる。トンネルの中と、入口の手前で移り終えられない位置では、車線を変えない。速度計は、前の車に合わせて落とした実際の速さを表示する。なしのときは、車線を変えない) | 自車と他車 |
| 複数のモニターを1つの画面として描く | Span all monitors as one screen | オン / オフ(オンのとき、スクリーンセーバーと`/play`で、すべてのモニターを1つの大きな画面とみなして、景色がモニターにまたがってつながるように描く。道路の先の消失点は、Windowsのメインディスプレイ(主モニター)の中央に置き、速度計と操作の案内は主モニターにだけ出す。主モニターが並びの端にあるときは、消失点から遠い側の絵が広く、近い側の絵は画面の外になる。オフのときは、モニターごとに別の景色を描く。モニターが1台のとき、プレビュー、`/window`には関係しない) | オフ |
| 表示言語 | Language | 自動(Windowsの表示言語) / English / 日本語 | 自動 |

表示言語は、設定画面とエラーメッセージの言語です。「自動」のときは、Windowsの表示言語が日本語なら日本語、それ以外は英語で表示します。設定画面で言語を選ぶと、その場で表示が切り替わります(保存するには「OK」を押します)。

設定は、レジストリの`HKEY_CURRENT_USER\Software\NightFreewaySS`に保存されます。

### 流しておく

スクリーンセーバーとしてではなく、しばらく流しておく方法が2つあります。マウスやキーの入力では、スクリーンセーバーのようには終わりません。

- 設定画面の「流しておく...」のボタンを押すと開くダイアログで、「ウィンドウで」か「全画面で」を選び、閉じるまでの分数(0は閉じない)を入れて「開始」を押します。設定画面の今の値を保存してから、両方のダイアログが閉じ、続けて選んだ形で始まります。この選択は保存されません
- `NightFreewaySS.scr`に引数を付けて起動します。ショートカットの「リンク先」の末尾に引数を足す方法などがあります

| 引数 | 動き |
| --- | --- |
| `/window` | 大きさを変えられる普通のウィンドウで流します(既定は1280×720。画面の中央に開き、収まらないときは縮みます)。閉じるボタン・Alt+F4・Escで終わります |
| `/play` | すべてのモニターに全画面で流します。EscかAlt+F4で終わります。マウスのカーソルは、動かしてから2秒だけ見えます |
| `/minutes:<n>` | `/window`か`/play`と一緒に使います。n分(1〜1440)たつと閉じます |
| `/span` | `/s`か`/play`と一緒に使います。その回だけ、設定にかかわらず、すべてのモニターを1つの画面として描きます |
| `/nospan` | `/s`か`/play`と一緒に使います。その回だけ、設定にかかわらず、モニターごとに別の景色を描きます |

例: `NightFreewaySS.scr /window /minutes:30`、`NightFreewaySS.scr /play /span`

引数は、大文字と小文字を区別せず、`/`と`-`のどちらでも始められ、順序は問いません。流しているあいだは、ディスプレイとシステムがスリープしません。終わると元に戻ります。Windowsがスクリーンセーバーとして起動するときは`/s`だけを渡すので、そのときは設定の値が使われます。

流しているあいだは、次のキーが使えます。

| キー | 動き |
| --- | --- |
| Esc | 終わります |
| ↑ / ↓ | 押しているあいだ、速度を上げ下げします。範囲は設定画面と同じ30〜160 km/hで、巡航速度のゆらぎは、新しい速度のまわりで続きます |
| Space | 一時停止と再開 |
| D | 左側通行と右側通行を切り替えます(画面が左右に反転します) |
| F | ウィンドウと全画面を切り替えます。`/play`でウィンドウにするときは、Fを押した画面のウィンドウだけが残ります。ウィンドウから全画面に戻すときは、モニターを1つの画面として描く設定(または`/span`)がオンなら、すべてのモニターの全画面に戻ります |

キーによる変更はその回だけのもので、設定には保存されません。複数のモニターに流しているときは、すべての画面に同じように効きます。画面の下の中央には、終わり方を含む操作の案内が出ます(モニターを1つの画面として描いているときは、主モニターにだけ出ます)。始めたときと、マウスを動かしたときやキーを押したときに出て、入力がないまま4秒たつと消えます。一時停止しているあいだは出たままになります。

### アンインストール

1. スクリーンセーバーの設定画面で、別のスクリーンセーバーまたは「(なし)」を選びます。
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
| Speedometer | None / Digits only / Dashboard (simple) / Dashboard (detailed) (digits only shows the speed as a number in the lower left corner; the simple dashboard shows an arc gauge with the speed; the detailed dashboard shows a speedometer with a scale and a needle, a tachometer, the time, the distance driven and the gear; a setting that showed the speedometer in an earlier version becomes digits only) | None |
| Show distant mountains | On / Off (when on, mountain ranges stand on the horizon in some directions, and red lights on some peaks blink slowly) | On |
| Vary the cruising speed | On / Off (when on, the speed slowly changes, over a minute or two at a time, within 20 km/h of the chosen speed; the speedometer shows the actual speed) | On |
| Traffic follows the time of day | On / Off (when on, the traffic follows the PC's time at startup: fewer cars and more trucks late at night and early in the morning, and as usual during the day) | On |
| Alternate mountains and plains | On / Off (when on, every 10 to 20 minutes at 100 km/h, mountain areas with mountain ranges and tunnels and plains with neither take turns) | On |
| Trucks | None / Japanese style / North American style (about one in five other vehicles is a truck; Japanese style has large box trucks and tractor-trailers with upper marker lamps and amber side markers, North American style has hooded semi-trailers and box trucks with identification and clearance lamps and side markers that are amber toward the front and red toward the rear; when the car bodies are Blocks, trucks are made of boxes with wheels, a cab and a cargo area; with Detailed shapes, the cab has a leaning front and the roof a deflector; and with Simple boxes each truck is one box that holds its whole shape) | Japanese style |
| Tunnels | None / Mixed lighting / Sodium (orange) / LED (white) (unless None, a tunnel that takes 20 to 60 seconds to drive through appears about once every 3 minutes at 100 km/h; Mixed lighting changes the lamp color from tunnel to tunnel; the sky is not seen inside a tunnel) | Mixed lighting |
| Car bodies | None (lights only) / Simple boxes / Blocks / Detailed shapes (with None, only the lights of the cars are drawn; with Simple boxes, Blocks or Detailed shapes, the bodies of the cars ahead, the passing cars and the oncoming cars, trucks included, are drawn as blackish surfaces, with a different shade for the end, the sides and the roof, that hide the lights behind them; Simple boxes draws every vehicle as one box that holds its whole shape; Blocks draws cars as sedans, hatchbacks, minivans and SUVs out of boxes (with the Japanese trucks setting, kei cars are added; with the North American one, pickup trucks), and trucks with a cab, a cargo area and wheels; Detailed shapes draws the same cars with outlines close to those of real ones, with a hood, a raked windshield and windows (a slightly bluer dark color than the body) and sides that narrow toward the roof, and the cabs of trucks with outlines too (cars far away are drawn as blocks); with bodies, the cars also cast shadows on the pools of light of the street lamps on the median and of the lamps in tunnels, each shadow shades only the pool of its own lamp and has a soft edge; if a version before 1.11 had the bodies on, they are Blocks) | None |
| Body color | Translucent black / Black (the color of the bodies when they are drawn; translucent black lets a tenth of the light behind them through, and black hides it completely; it is not used, and cannot be chosen in the settings dialog, while the car bodies are None) | Translucent black |
| Lane changes | None / Own car only / Own car and other cars (with own car only, when your car catches up with a slower car ahead, it signals for 3 seconds, moves into the passing lane if that is free, passes, and returns to its lane; if the passing lane is not free, it keeps a distance behind the car ahead and passes when the lane is free; with own car and other cars, the other cars change lanes in the same way; a car that changes lanes blinks an amber turn signal on the rear corner on the side it moves to; while your car moves between lanes, the view turns slightly in the direction of the move; no car starts a lane change inside a tunnel or where it could not finish before the entrance; the speedometer shows the actual speed, which falls behind the car ahead; with none, no car changes lanes) | Own car and other cars |
| Span all monitors as one screen | Span all monitors as one screen | On / Off (when on, the screensaver and `/play` treat all the monitors as one large screen and draw the scenery so that it continues across them; the vanishing point at the end of the road is in the middle of the primary monitor, the main display of Windows, and the speedometer and the line about the keys appear only on the primary monitor; when the primary monitor is at the end of the row, the picture on the side farther from the vanishing point is wider, and that on the nearer side is outside the screen; when off, each monitor draws its own scenery; it has no effect with one monitor, in the preview or with `/window`) | Off |
| Language | Automatic (Windows display language) / English / 日本語 | Automatic |

The language is used for the settings dialog and error messages. With "Automatic", Japanese is shown when the Windows display language is Japanese, and English otherwise. When you choose a language in the dialog, the display changes at once (click "OK" to save it).

The settings are saved in the registry under `HKEY_CURRENT_USER\Software\NightFreewaySS`.

### Keeping it running

There are two ways to keep the road running as something other than a screensaver. Keyboard and mouse input does not end it the way it ends a screensaver.

- Click "Keep running..." in the settings dialog. In the dialog that opens, choose "In a window" or "Full screen", enter the minutes after which it closes (0 means it does not close) and click "Start". The current values of the settings dialog are saved, both dialogs close and the road starts in the chosen form. This choice is not saved
- Start `NightFreewaySS.scr` with an argument, for example by adding it to the end of the "Target" of a shortcut

| Argument | Behavior |
| --- | --- |
| `/window` | Runs in an ordinary window that can be resized (1280 by 720 by default; it opens in the middle of the screen and shrinks if it does not fit). The close button, Alt+F4 and Esc end it |
| `/play` | Runs full screen on every monitor. Esc and Alt+F4 end it. The mouse cursor is seen for 2 seconds after the mouse moved |
| `/minutes:<n>` | Goes with `/window` or `/play`. Closes it after n minutes (1 to 1440) |
| `/span` | Goes with `/s` or `/play`. For that run only, draws all the monitors as one screen, whatever the setting is |
| `/nospan` | Goes with `/s` or `/play`. For that run only, draws separate scenery on each monitor, whatever the setting is |

Examples: `NightFreewaySS.scr /window /minutes:30`, `NightFreewaySS.scr /play /span`

Arguments are not case-sensitive, may start with `/` or `-`, and may come in any order. While it runs, the display and the system do not go to sleep; that is undone when it ends. When Windows starts the screensaver, it passes only `/s`, so the setting is used then.

While it runs, these keys work.

| Key | Behavior |
| --- | --- |
| Esc | Ends it |
| Up / Down | Raise or lower the speed while the key is held. The range is 30 to 160 km/h, as in the settings dialog, and the slow variation of the speed goes on around the new speed |
| Space | Pauses and resumes |
| D | Switches between left-hand and right-hand traffic (the picture turns over from left to right) |
| F | Switches between a window and full screen. When going from `/play` to a window, only the window of the screen where F was pressed stays. When going back to full screen, it takes all the monitors again if the setting to draw the monitors as one screen (or `/span`) is on |

The changes made with the keys last only for that run and are not saved in the settings. With several monitors, they act on every screen alike. A line about the keys, including how to end it, appears at the bottom center of the picture (only on the primary monitor when the monitors are drawn as one screen). It appears when the run starts and when the mouse moves or a key is pressed, and goes away after 4 seconds without input. While paused, it stays.

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
