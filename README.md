# Night Freeway SS

夜の高速道路を光の点で描くWindowsスクリーンセイバー。mhatta氏の[nightfreeway](https://github.com/mhatta/nightfreeway)(ブラウザで動くアニメーション)を参考に、挙動と見た目を似せてRustで独自に実装する。本プロジェクトは本家とは別のプロジェクトである。

## 状態

開発中(Phase 0: 開発基盤)。まだスクリーンセイバーとしては動作しない。計画と進捗は[実装計画](docs/plans/0001-screensaver-implementation-plan.md)、技術判断は[ADR](docs/adr/README.md)を参照。

## 構成

| パス | 内容 |
| --- | --- |
| `crates/freeway-core` | OS非依存のシーン計算(道路・背景・交通をPhase 2以降で実装予定)。すべてのプラットフォームでテストできる |
| `crates/freeway-scr` | Windows固有部分(引数処理・ウィンドウ・Direct2D描画・設定画面をPhase 1以降で実装予定)。配布物の`.scr`になる |
| `docs/plans/` | 実装計画 |
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

Windows固有のコードはWindows以外の環境(Linuxで確認済み)でも`--target x86_64-pc-windows-msvc`付きの`cargo check`で型チェックできる。配布用のバイナリはWindows上でビルドする。

AIコーディングエージェントへの指示は`AGENTS.md`にある。

## 謝辞

本プロジェクトは、mhatta氏による[nightfreeway](https://github.com/mhatta/nightfreeway)(Copyright (c) 2026 Masayuki Hatta、MITライセンス)の挙動と見た目を参考にしている。コードは移植せず独自に実装している。

nightfreewayの謝辞にならい、原作のFreeWayの作者であるFKS氏、Windows移植を手がけたYoshihiro Sugahara氏、nightfreewayが参考にしたsoyokaze2000.comのFreeWay再現版にも感謝する。

## ライセンス

[MIT](LICENSE)
