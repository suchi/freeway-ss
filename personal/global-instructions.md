# 個人グローバルインストラクション

全リポジトリ共通の個人設定。リポジトリに`AGENTS.md`がある場合はそちらを優先する。

## 進め方

- 人間が判断するAgentic Codingで進める。曖昧な点は勝手に判断せず、選択肢とpros/consを提示して確認する。
- 破壊的操作(force push・`reset --hard`・削除)とGit操作(コミット・プッシュ・マージ)の前に確認を取る。
- 私の変更を明示的な依頼なしに巻き戻さない。
- 想定外の挙動は原因を特定してから対処する。原因不明のままワークアラウンドで回避しない。
- 「完了」はCI通過・動作確認後にのみ報告する。未確認なら「プッシュ済み、CIの確認待ち」と言う。

## 言語

- 日本語: 会話・説明・ドキュメント・PR/Issue本文・コミットメッセージ本文。
- 英語: PR/Issueタイトル・コミットメッセージ1行目・識別子・ブランチ名・コード内コメント・ログ/エラー/UIラベル。
- 図はMermaidで書く(ASCIIアート禁止)。

## 開発スタイル

- 1コミット=1論理変更。Conventional Commitsに従う。
- `main`へ直接コミットしない。ブランチ+PRで進める。
- TDD(t_wada流): Red → Green → Refactor。

## ツール・環境

- Pythonはpipではなくuvを使う(`uv sync --locked`・`uv run pytest`。CI/CDでは必ず`--locked`)。
- 依存追加は慎重に。サプライチェーン対策(Dependabotの`cooldown`・uvの`exclude-newer = "7d"`)を私の承認なしに外さない。
- 主環境はWindows 11(PowerShell)とWSL2。パス・コマンド例はどちらの環境かを確認してから提示する。
- WindowsネイティブではMicrosoft Core Utils(公式coreutils実装)を利用する前提: https://learn.microsoft.com/ja-jp/windows/core-utils/overview

## 新しいリポジトリを始めるとき

- リポジトリに`AGENTS.md`がなければ、エージェント設定テンプレートリポジトリからの導入を提案する。
