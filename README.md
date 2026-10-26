# Night Freeway SS

夜の高速道路を光の点で描くWindowsスクリーンセイバー。mhatta氏の[nightfreeway](https://github.com/mhatta/nightfreeway)(ブラウザで動くアニメーション)を参考に、挙動と見た目を似せてRustで独自に実装する。本プロジェクトは本家とは別のプロジェクトである。

## 状態

正式版v1.0.0を公開済み(x64とARM64)。任意機能(Phase 7)を1つずつ追加している(7.1: UFO、7.2: 設定画面の多言語化)。スクリーンセイバーとして起動・プレビュー・終了でき、カーブと起伏のある夜の高速道路(車線・デリネーター・街灯・路面の光)と、夜空(またたく星・街明かり・地平線の空の明るみ・流れ星)を描く。背景はカーブに合わせて横に流れる。同じ向きの車の尾灯とブレーキ灯、追い越していく車、対向車の前照灯を描く。設定画面(巡航速度・通行方向・速度計の表示・UFOの表示・表示言語)から設定を変更でき、保存した値は次の起動から反映される。計画と進捗は[実装計画](docs/plans/0001-screensaver-implementation-plan.md)、技術判断は[ADR](docs/adr/README.md)を参照。

## 構成

| パス | 内容 |
| --- | --- |
| `crates/freeway-core` | OS非依存のシーン計算(道路形状・投影・道路の光・背景・他車・設定値の検証を実装済み)。すべてのプラットフォームでテストできる |
| `crates/freeway-scr` | Windows固有部分(引数処理・ウィンドウ・Direct2D描画・設定画面とレジストリへの保存。ダイアログ・マニフェスト・表示名・アイコンは`freeway.rc`から`build.rs`で埋め込む)。配布物の`.scr`になる |
| `packaging/` | 配布用zipに入れる、利用者向けの説明書(`README.md`。日本語と英語)と、`.scr`に組み込まれる第三者のソフトウェアの著作権表示(`THIRD-PARTY-NOTICES.txt`)。本ファイルは開発者向けで、zipには入れない |
| `xtask` | 開発用コマンド(`cargo xtask dist`: 配布物の作成、`cargo xtask icon`: アイコンの生成)。標準ライブラリだけで書いている |
| `.github/workflows/` | CI(`ci.yml`)と、`v*`のタグのプッシュで成果物をGitHub Releaseに公開するワークフロー(`release.yml`。手順は`AGENTS.md`) |
| `docs/plans/` | 実装計画(進捗表を含む)とフェーズの詳細設計 |
| `docs/adr/` | アーキテクチャ決定記録 |

## インストール

1. [GitHub Releases](https://github.com/suchi/freeway-ss/releases)から、お使いのPCに合うzipを取得する。
   - `NightFreewaySS-v<バージョン>-win-x64.zip`: 一般的なIntel/AMDのPC
   - `NightFreewaySS-v<バージョン>-win-arm64.zip`: ARM64のPC(「設定」の「システム」「バージョン情報」で、システムの種類が「ARMベースのプロセッサ」と表示される)
2. 取得したzipの整合性を確かめたい場合は、同じリリースの`SHA256SUMS.txt`の値と、PowerShellの`Get-FileHash <zipのパス>`の結果を比べる(`Get-FileHash`は16進を大文字で表示するが、大文字・小文字の違いは無視してよい)。
3. zipを、スクリーンセイバーを置いておくフォルダーに展開する。
4. 展開した`NightFreewaySS.scr`を右クリックして「インストール」を選ぶ。スクリーンセイバーの設定画面が開き、一覧に「Night Freeway SS」が現れる。Windowsの「インストール」はファイルをコピーせず、展開した場所の`.scr`をそのまま登録する(`HKEY_CURRENT_USER\Control Panel\Desktop`の`SCRNSAVE.EXE`)ため、このファイルを移動・削除するとスクリーンセイバーは動かなくなる。
5. 設定画面の「設定」から、巡航速度・通行方向・速度計の表示・UFOの表示・表示言語を変える(下の「設定」を参照)。

未署名のため、Microsoft Defender SmartScreenの警告が出ることがある。配布元を確認したうえで、警告の「詳細情報」から実行できる。

## アンインストール

1. スクリーンセイバーの設定画面で、別のスクリーンセイバー(または「なし」)を選ぶ。
2. zipを展開したフォルダーごと削除する。
3. 保存した設定を消す(下の「設定」の`reg delete`を参照)。

## 開発

必要なもの: [rustup](https://rustup.rs/)。ツールチェーンのバージョンとWindowsターゲットは`rust-toolchain.toml`で固定している。

```sh
rustup toolchain install
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo check --workspace --all-targets --target x86_64-pc-windows-msvc
cargo build --release -p freeway-scr
```

Windowsでのビルドには、リソースコンパイラ(`rc.exe`。Visual Studio Build ToolsのWindows SDKに含まれる)が必要である([ADR-0005](docs/adr/0005-embed-windows-resources-with-embed-resource.md))。ARM64向けのビルドには、Visual Studio Build ToolsのARM64用コンポーネントも必要である。

Windows固有のコードはWindows以外の環境(Linuxで確認済み)でも`--target x86_64-pc-windows-msvc`付きの`cargo check`で型チェックできる。配布用のバイナリはWindows上でビルドする。Windows向けのビルドではC runtimeを静的リンクする(`.cargo/config.toml`)。

### Windowsで試す

配布物と同じものを作るには、Windowsで`cargo xtask dist`を実行する。リリースビルドの後、次のファイルが`target\dist\`に作られる([ADR-0006](docs/adr/0006-build-release-artifacts-with-cargo-xtask.md))。

| ファイル | 内容 |
| --- | --- |
| `x64\NightFreewaySS.scr` | そのまま使える`.scr`(ARM64は`arm64\`) |
| `NightFreewaySS-v<バージョン>-win-x64.zip` | `.scr`・LICENSE・第三者の著作権表示(`packaging/THIRD-PARTY-NOTICES.txt`)・利用者向けの説明書(`packaging/README.md`を`README.md`として格納)をまとめた配布用zip(ARM64は`win-arm64`) |
| `SHA256SUMS.txt` | zipのSHA-256(`sha256sum -c`の書式。複数のアーキテクチャを順に作ると追記される) |

ARM64向けは`cargo xtask dist --target aarch64-pc-windows-msvc`で作る。`.scr`はWindows以外ではビルドできない(`cargo xtask dist`はその旨のエラーで終わる)。

動かし方は次のとおり。

- `NightFreewaySS.scr /s`: 全画面で実行する。キー入力・クリック・マウス移動で終了する
- `NightFreewaySS.scr /c`(または`.scr`のダブルクリック): 設定画面を開く
- `.scr`ファイルを右クリックして「インストール」: スクリーンセイバーの設定画面に追加され、プレビューと待機時間後の起動を確認できる

UFOは、設定をオンにしても平均10分に1回しか現れない。実機で確かめるときは、Cargoのフィーチャー`ufo-demo`を付けてビルドする(`cargo build --release -p freeway-scr --features ufo-demo`)。平均の間隔が20秒になる。配布物(`cargo xtask dist`)とリリースには使わない。

アイコンを変えるときは、`xtask/src/icon.rs`の図柄を直して`cargo xtask icon`を実行し、生成された`crates/freeway-scr/resources/freeway.ico`をコミットする(`cargo test`が、コミット済みの`.ico`が生成結果と一致することを確かめる)。

## 設定

設定画面の項目と、保存先のレジストリ値(`HKEY_CURRENT_USER\Software\NightFreewaySS`、すべて`REG_DWORD`)は次のとおりである。値がない・型が違う・範囲外のときは、その項目だけ既定値で動く。

| 項目 | レジストリ値 | 範囲 | 既定値 |
| --- | --- | --- | --- |
| 巡航速度 | `SpeedKmh` | 30〜160 km/h(10 km/h刻み) | 100 |
| 通行方向 | `LeftHandTraffic` | 左側通行は1、右側通行は0 | 1 |
| 速度計を表示 | `ShowSpeedometer` | 表示は1、非表示は0 | 0 |
| UFOを時々表示 | `ShowUfo` | 表示は1、非表示は0 | 0 |
| 表示言語 | `Language` | 自動は0、Englishは1、日本語は2 | 0 |

表示言語は、設定画面とエラーメッセージの言語である。自動のときは、Windowsの表示言語が日本語なら日本語、それ以外は英語で表示する。設定画面で選ぶと、その場で表示が切り替わる。

設定はスクリーンセイバーを削除しても残る。設定を消すには、レジストリエディターで上記のキーを削除するか、次のコマンドを実行する。

```bat
reg delete HKCU\Software\NightFreewaySS /f
```

AIコーディングエージェントへの指示は`AGENTS.md`にある。

## 謝辞

本プロジェクトは、mhatta氏による[nightfreeway](https://github.com/mhatta/nightfreeway)(Copyright (c) 2026 Masayuki Hatta、MITライセンス)の挙動と見た目を参考にしている。コードは移植せず独自に実装している。

nightfreewayの謝辞にならい、原作のFreeWayの作者であるFKS氏、Windows移植を手がけたYoshihiro Sugahara氏、nightfreewayが参考にしたsoyokaze2000.comのFreeWay再現版にも感謝する。

## ライセンス

[MIT](LICENSE)

## 免責事項

本ソフトウェアは、AIコーディングエージェントにコードの作成の大部分を任せる、いわゆるVibe Codingで開発した。作者は設計の判断と実機での動作確認を行っているが、すべてのコードを詳細に検証しているわけではない。

そのため、作者は本ソフトウェアの品質・安全性・動作を保証せず、本ソフトウェアの利用によって生じた不具合・損害・トラブルについて責任を負わない。これは[MITライセンス](LICENSE)の無保証の条項を、改めて明示するものである。
