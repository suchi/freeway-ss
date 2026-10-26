# ADR-0005: embed-windows-resources-with-embed-resource

- ステータス: 採用
- 日付: 2026-10-26

## 背景

Phase 5で、スクリーンセイバーの設定画面(`/c`)を作る。設定画面をWindows標準の見た目(ビジュアルスタイル)で表示するには、アプリケーションマニフェストでCommon Controls v6を指定する必要がある。またPhase 6では、スクリーンセイバーの一覧に出る表示名やアイコンを実行ファイルのリソースとして持たせる必要がある。Rustの標準のビルドでは、Windowsのリソース(`.rc`)を実行ファイルに埋め込めない。

## 決定

ビルド時の依存として`embed-resource`(3.0系、crates.ioで実在とリポジトリ`nabijaczleweli/rust-embed-resource`を確認、MITライセンス)を`freeway-scr`に追加し、ビルドスクリプト(`build.rs`)で`.rc`ファイルをコンパイルして実行ファイルに埋め込む。

- 設定画面の配置はダイアログテンプレート(`.rc`)で定義する
- マニフェストで、Common Controls v6、DPI認識(Per-Monitor v2)、対応OS(Windows 10以降)を指定する
- リソースのコンパイルは、ターゲットがWindowsのときだけ行う。Linux上でのWindowsターゲットの型チェック(`cargo check --target x86_64-pc-windows-msvc`、AGENTS.mdの検証コマンド)は、リソースコンパイラがなくても失敗しないようにする
- 実行時の依存は増えない(ビルド時のみの依存)

検討した選択肢と選ばなかった理由:

- 依存を追加せず、ダイアログをコードで組み立てる(メモリ上のテンプレートや`CreateWindowExW`): 依存は増えないが、マニフェストがないため古い見た目になり、配置のコードも増える。Phase 6で表示名とアイコンを埋め込むときに、結局同じ判断が必要になる
- `winresource`: 同じ目的のクレートだが、`embed-resource`のほうが利用実績(ダウンロード数)が多く、マニフェストの扱いを明示的に選べる(`manifest_optional`/`manifest_required`)

## 結果

- メリット: 設定画面がWindows標準の見た目になる。ダイアログの配置を`.rc`で宣言的に書ける。Phase 6の表示名・アイコンの埋め込みにも同じ仕組みを使える
- デメリット・受け入れたトレードオフ: ビルド時の依存が1つ増える。Windowsでのビルドにリソースコンパイラ(Windows SDKの`rc.exe`。Visual Studio Build Toolsに含まれる)が必要になる
- 将来の見直し条件: `embed-resource`の保守が止まった場合、またはRust・Cargoが標準でリソースの埋め込みに対応した場合

参照: [Phase 5の詳細設計](../plans/0005-phase5-settings.md)、[ADR-0004](0004-store-settings-in-registry.md)
