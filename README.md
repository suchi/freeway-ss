# Night Freeway SS

夜の高速道路を光の点で描くWindowsスクリーンセイバー。mhatta氏の[nightfreeway](https://github.com/mhatta/nightfreeway)(ブラウザで動くアニメーション)を参考に、挙動と見た目を似せてRustで独自に実装する。本プロジェクトは本家とは別のプロジェクトである。

## 状態

開発中(Phase 3: 背景)。スクリーンセイバーとして起動・プレビュー・終了でき、カーブと起伏のある夜の高速道路(車線・デリネーター・街灯・路面の光)と、夜空(またたく星・街明かり・地平線の空の明るみ・流れ星)を描く。背景はカーブに合わせて横に流れる。他車はまだ描かない。設定画面も未実装で、設定ボタンでは案内を表示するだけである。計画と進捗は[実装計画](docs/plans/0001-screensaver-implementation-plan.md)、技術判断は[ADR](docs/adr/README.md)を参照。

## 構成

| パス | 内容 |
| --- | --- |
| `crates/freeway-core` | OS非依存のシーン計算(道路形状・投影・道路の光・背景を実装済み。交通はPhase 4以降で実装予定)。すべてのプラットフォームでテストできる |
| `crates/freeway-scr` | Windows固有部分(引数処理・ウィンドウ・Direct2D描画。設定画面はPhase 5で実装予定)。配布物の`.scr`になる |
| `.github/workflows/` | CI(`ci.yml`)と、フェーズごとの成果物をGitHub Releaseに公開する手動実行のワークフロー(`release.yml`。手順は`AGENTS.md`) |
| `docs/plans/` | 実装計画(進捗表を含む)とフェーズの詳細設計 |
| `docs/adr/` | アーキテクチャ決定記録 |

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

Windows固有のコードはWindows以外の環境(Linuxで確認済み)でも`--target x86_64-pc-windows-msvc`付きの`cargo check`で型チェックできる。配布用のバイナリはWindows上でビルドする。Windows向けのビルドではC runtimeを静的リンクする(`.cargo/config.toml`)。

### Windowsで試す

1. `cargo build --release -p freeway-scr`でビルドする。
2. `target\release\freeway-scr.exe`を`NightFreewaySS.scr`という名前でコピーする。
3. 次のいずれかで動かす。
   - `NightFreewaySS.scr /s`: 全画面で実行する。キー入力・クリック・マウス移動で終了する
   - `.scr`ファイルを右クリックして「インストール」: スクリーンセイバーの設定画面に追加され、プレビューと待機時間後の起動を確認できる

AIコーディングエージェントへの指示は`AGENTS.md`にある。

## 謝辞

本プロジェクトは、mhatta氏による[nightfreeway](https://github.com/mhatta/nightfreeway)(Copyright (c) 2026 Masayuki Hatta、MITライセンス)の挙動と見た目を参考にしている。コードは移植せず独自に実装している。

nightfreewayの謝辞にならい、原作のFreeWayの作者であるFKS氏、Windows移植を手がけたYoshihiro Sugahara氏、nightfreewayが参考にしたsoyokaze2000.comのFreeWay再現版にも感謝する。

## ライセンス

[MIT](LICENSE)
