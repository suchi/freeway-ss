# 開発者向けガイド

Night Freeway SSの構成・ビルド・テスト・配布物の作り方・設定の保存先をまとめる。開発の進め方と、最初に読む文書の順番は[オンボーディング](onboarding.md)にある。

## 構成

| パス | 内容 |
| --- | --- |
| `crates/freeway-core` | OS非依存のシーン計算(道路の形・投影・道路の光・空・山並み・トンネル・他車と車体(`traffic.rs`が車と箱、`solid.rs`が輪郭の立体)・自車の追い越し(`ego.rs`)・設定値の検証・画面の文字列。複数のモニターにまたがる絵の1つの窓の分は`Scene::draw_span`で描く)。すべてのプラットフォームでテストできる |
| `crates/freeway-scr` | Windows固有の部分(引数の処理・ウィンドウの位置と大きさ、複数のモニターを1つの画面として描くときの配置(`placement.rs`)・キーの判定と操作の案内の時間(`input.rs`)・ウィンドウ・Direct2Dでの描画・設定画面とレジストリへの保存。ダイアログ・マニフェスト・表示名・アイコン・版の情報は`freeway.rc`から`build.rs`で埋め込む)。配布物の`.scr`になる |
| `packaging/` | 配布用のzipに入れる、利用者向けの説明書(`README.md`。日本語と英語)と、`.scr`に組み込まれる第三者のソフトウェアの著作権表示(`THIRD-PARTY-NOTICES.txt`)。リポジトリの最上位の`README.md`は、zipには入れない |
| `xtask` | 開発用のコマンド(`cargo xtask dist`: 配布物の作成、`cargo xtask icon`: アイコンの生成)。標準ライブラリだけで書いている |
| `.github/workflows/` | CI(`ci.yml`)と、`v*`のタグのプッシュで成果物をGitHub Releaseに公開するワークフロー(`release.yml`。手順は`AGENTS.md`の「リリース」) |
| `.claude/` | Claude Code用の設定・フック・スキル。フックは、危険なgit操作の拒否、設定ファイルの変更時の確認、英語の応答の書き直しを行う |
| `docs/plans/` | 実装計画(進捗表を含む)と、フェーズ・リリースごとの詳細設計 |
| `docs/adr/` | アーキテクチャ決定記録 |

## 開発環境

必要なもの: [rustup](https://rustup.rs/)。ツールチェーンのバージョンとWindowsのターゲットは、`rust-toolchain.toml`で固定している。

```sh
rustup toolchain install
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo check --workspace --all-targets --target x86_64-pc-windows-msvc
cargo build --release -p freeway-scr
```

コミットの前に実行するコマンドの一覧は、`AGENTS.md`の「検証コマンド」が正である。

Windowsでのビルドには、リソースコンパイラ(`rc.exe`。Visual Studio Build ToolsのWindows SDKに含まれる)が必要である([ADR-0005](adr/0005-embed-windows-resources-with-embed-resource.md))。ARM64向けのビルドには、Visual Studio Build ToolsのARM64用のコンポーネントも必要である。

Windows固有のコードは、Windows以外の環境(Linuxで確認済み)でも、`--target x86_64-pc-windows-msvc`付きの`cargo check`で型チェックできる。配布用のバイナリはWindows上でビルドする。Windows向けのビルドでは、C runtimeを静的リンクする(`.cargo/config.toml`)。

## Windowsで試す

配布物と同じものを作るには、Windowsで`cargo xtask dist`を実行する。リリースビルドの後、次のファイルが`target\dist\`に作られる([ADR-0006](adr/0006-build-release-artifacts-with-cargo-xtask.md))。

| ファイル | 内容 |
| --- | --- |
| `x64\NightFreewaySS.scr` | そのまま使える`.scr`(ARM64は`arm64\`) |
| `NightFreewaySS-v<バージョン>-win-x64.zip` | `.scr`・LICENSE・第三者の著作権表示(`packaging/THIRD-PARTY-NOTICES.txt`)・利用者向けの説明書(`packaging/README.md`を`README.md`として格納)をまとめた配布用のzip(ARM64は`win-arm64`) |
| `SHA256SUMS.txt` | zipのSHA-256(`sha256sum -c`の書式。複数のアーキテクチャを順に作ると追記される) |

ARM64向けは`cargo xtask dist --target aarch64-pc-windows-msvc`で作る。`.scr`はWindows以外ではビルドできない(`cargo xtask dist`はその旨のエラーで終わる)。

動かし方:

- `NightFreewaySS.scr /s`: 全画面で実行する。キー入力・クリック・マウスの移動で終了する
- `NightFreewaySS.scr /c`(または`.scr`のダブルクリック): 設定画面を開く
- `NightFreewaySS.scr /window`: 大きさを変えられるウィンドウで流す(既定は1280×720、作業領域の中央)。入力では終わらず、閉じるボタン・Alt+F4・Escで終わる
- `NightFreewaySS.scr /play`: すべてのモニターで全画面で流す。EscかAlt+F4だけで終わる。カーソルは、マウスを動かしてから2秒だけ見せる
- `/span`・`/nospan`: `/s`か`/play`と一緒に使い、その回だけ、設定`SpanMonitors`にかかわらず、すべてのモニターを1つの画面として描く(`/span`)か、モニターごとに独立した場面を描く(`/nospan`)。例: `NightFreewaySS.scr /play /span`。仕組みは次の「複数のモニターを1つの画面として描く」にある
- `/window`と`/play`のキー操作(0021の3.2節。`/s`とプレビューでは効かない): ↑↓は押しているあいだ選んだ速度を8 m/s²で上げ下げし(30〜160 km/h)、Spaceは一時停止と再開、Dは通行方向の切り替え(時定数1/3秒で左右を反転する)、Fはウィンドウと全画面の切り替え、コナミコマンド(↑↑↓↓←→←→BA。直近の10個のキーで判定し、押しっぱなしの繰り返しは数えない)はUFOを呼ぶ(設定がオフでも現れる)。変更はその回だけで、設定には保存しない。複数のモニターでは、すべての窓の場面に同じように効く。Fは同じ窓のスタイルを替えるので場面は続き、`/play`からウィンドウにするときは、Fを受けた窓だけを残してほかを閉じる。画面の下の中央に操作の案内(Esc・↑↓・Space・D・F。コナミコマンドは書かない)を、始めたときと入力のときに出し、入力がないまま4秒たつと0.5秒で消す
- `/minutes:<n>`: `/window`か`/play`と一緒に使い、n分(1〜1440)で閉じる。例: `NightFreewaySS.scr /window /minutes:30`。流しているあいだは`SetThreadExecutionState`でスリープを止める。設定画面の「流しておく...」のボタンで開くダイアログの「開始」は、設定を保存し、両方のダイアログを閉じてから、自分自身をこの引数で別のプロセスとして起動する(この選択は保存しない)
- `.scr`を右クリックして「インストール」: スクリーンセーバーの設定画面に加わり、プレビューと、待機時間の後の起動を確かめられる。Windowsの「インストール」はファイルをコピーせず、展開した場所の`.scr`をそのまま登録する(`HKEY_CURRENT_USER\Control Panel\Desktop`の`SCRNSAVE.EXE`)

UFOは、設定をオンにしても平均10分に1回しか現れない。実機で確かめるときは、Cargoのフィーチャー`ufo-demo`を付けてビルドする(`cargo build --release -p freeway-scr --features ufo-demo`)。平均の間隔が20秒になる。配布物(`cargo xtask dist`)とリリースには使わない。

版の情報(`.scr`のプロパティの「詳細」に出る製品名・ファイルのバージョンなど)は、`freeway.rc`のVERSIONINFOに入れる。バージョンの数字は`Cargo.toml`の`workspace.package.version`だけに書き、`build.rs`が`CARGO_PKG_VERSION*`からマクロとして`rc.exe`に渡す。`cargo xtask dist`は、組み込まれたFileVersionとProductVersionが一致しない実行ファイルを配布物にしない(テスト用の実行ファイルにはリソースが入らないので、ビルドした実行ファイルで確かめる)。

アイコンを変えるときは、`xtask/src/icon.rs`の図柄を直して`cargo xtask icon`を実行し、生成された`crates/freeway-scr/resources/freeway.ico`をコミットする(`cargo test`が、コミット済みの`.ico`が生成結果と一致することを確かめる)。

## 複数のモニターを1つの画面として描く

設定`SpanMonitors`か`/span`がオンの`/s`・`/play`では、1つの場面([ADR-0008](adr/0008-span-monitors-as-one-scene.md))を、モニターごとの窓で分けて描く。オフのとき、モニターが1台のとき、プレビューと`/window`は、モニターごとに独立した場面を描く([ADR-0003](adr/0003-render-independent-scene-per-monitor.md))。詳細設計は[0023](plans/0023-v1-16-span-monitors.md)にある。

- 配置の計算(OSに依らない): `placement::span_layout`が、モニターの長方形(仮想デスクトップの物理画素)と主モニターから、それらを囲む長方形(仮想画面)の大きさ、焦点距離、消失点(主モニターの中央)、各モニターの部分を求める。窓の位置は、毎フレーム、実際の窓から求め直す
- 描画(`freeway-core`): `Scene::draw_span`が、`Span`(仮想画面・消失点・描く部分)を受け取り、その部分を、部分の左上を原点とする座標で描く。焦点距離は仮想画面の大きさから`projection::focal_length`で決まり、部分の外にある光は描かない。1画面の描画`Scene::draw`は`Span::whole`を通すだけである
- 窓(`win/mod.rs`): `Scene::update`は1フレームに1回で、すべての窓が同じ場面を描く。速度計と操作の案内は、主モニターの窓にだけ出す。垂直同期は、1フレームに1回(最後の窓)だけ待つ

## 設定の保存先

設定は、レジストリの`HKEY_CURRENT_USER\Software\NightFreewaySS`に、すべて`REG_DWORD`で保存する([ADR-0004](adr/0004-store-settings-in-registry.md))。値がない・型が違う・範囲外のときは、その項目だけ既定値で動く。各項目の意味は、利用者向けの[説明書](../packaging/README.md)にある。

| 項目 | レジストリ値 | 範囲 | 既定値 |
| --- | --- | --- | --- |
| 巡航速度 | `SpeedKmh` | 30〜160 km/h(10 km/h刻み) | 100 |
| 通行方向 | `LeftHandTraffic` | 左側通行は1、右側通行は0 | 1 |
| 速度計 | `Speedometer` | なしは0、数字だけは1、ダッシュボード(シンプル)は2、ダッシュボード(作り込み)は3 | 0 |
| UFOを時々表示 | `ShowUfo` | 表示は1、非表示は0 | 0 |
| 遠くの山並みを表示 | `ShowMountains` | 表示は1、非表示は0 | 1 |
| 巡航速度のゆらぎ | `VarySpeed` | オンは1、オフは0 | 1 |
| 時刻に応じた交通量 | `TrafficByTime` | オンは1、オフは0 | 1 |
| 景色の区間(山あいと平野) | `SceneryZones` | オンは1、オフは0 | 1 |
| トラック | `Trucks` | なしは0、日本風は1、北米風は2 | 1 |
| トンネル | `Tunnels` | なしは0、混在は1、ナトリウム灯は2、LEDは3 | 1 |
| 他車の車体の形 | `CarShapes` | なし(灯火だけ)は0、シンプルな箱は1、ブロックは2、詳細な形は3 | 0 |
| 他車の車体の色 | `CarColor` | 半透明の黒は0、黒は1 | 0 |
| 車線変更 | `LaneChanges` | なしは0、自車だけは1、自車と他車は2 | 2 |
| 複数のモニターを1つの画面として描く | `SpanMonitors` | オンは1、オフは0 | 0 |
| 表示言語 | `Language` | 自動は0、Englishは1、日本語は2 | 0 |

`Speedometer`がないときは、前の版(1.10より前)の`ShowSpeedometer`が1なら数字だけ、それ以外はなしとして読む。`Speedometer`が範囲外のときは既定値で動く。保存するときは、前の版に戻したときにも表示のオン・オフが残るよう、`ShowSpeedometer`も「なし以外なら1」で書く。

`CarShapes`がないときは、前の版(1.11より前)の`CarBodies`から読む。0(透明)はなし、1(半透明の黒)はブロックと半透明の黒、2(黒)はブロックと黒になる(v1.12.0で、v1.11.0の「それらしい形」(値2)を「ブロック」と呼ぶことにした。値は変わらない)。`CarShapes`が範囲外のときは形だけが、`CarColor`が範囲外・ないときは色だけが既定値になる。保存するときは、前の版に戻したときにも車体の有無が残るよう、`CarBodies`も書く(形がなしなら0、それ以外は色に応じて1か2)。

`LaneChanges`は、ないときと範囲外のときに2(自車と他車)で動く。前の版に戻したときは、この値が読まれないだけである。車線変更の仕組み(自車の`ego.rs`、他車と重ならないための処理・ウインカーの`traffic.rs`、視点の向きの`projection.rs`)は[0022](plans/0022-v1-15-overtaking-and-lane-changes.md)にある。

時間的な変化の3項目(巡航速度のゆらぎ・時刻に応じた交通量・景色の区間)は、3つともオフにすると従来と同じ動きになる(詳細は[0013](plans/0013-phase7-6-time-variation.md))。

保存した設定を消すコマンド:

```bat
reg delete HKCU\Software\NightFreewaySS /f
```

## リリース

`v*`のタグのプッシュで、`release.yml`がx64とARM64の配布物を作り、GitHub Releaseに公開する。バージョンの上げ方・リリース前の点検・タグの付け方は、`AGENTS.md`の「リリース」が正である。
