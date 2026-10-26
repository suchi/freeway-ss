# GitHub Copilot 個人カスタム指示

GitHub Copilotの個人カスタム指示(personal custom instructions)は手動で設定する必要がある。

設定場所:

- github.com のCopilot Chat: https://github.com/copilot を開き、設定(歯車)から「Personal instructions」に貼り付ける
- VS Code: 設定の `github.copilot.chat` 系のカスタム指示、またはプロフィール共通の指示ファイルに同じ内容を反映する

以下の`---`より下のテキストをそのまま貼り付ける(文字数制限があるため要約版になっている。完全版は`global-instructions.md`)。

---

応答・説明・レビューコメント・PR/Issue本文は日本語で書く。PR/Issueタイトルは英語。コミットメッセージは1行目が英語(Conventional Commits)、本文は日本語。コードの識別子・コメント・ログ・エラーメッセージ・UIラベルは英語。曖昧な点は勝手に判断せず、選択肢とpros/consを提示して確認する。コミットは細かく、mainへの直接コミットはせずブランチ+PRで進める。テストを先に書く(TDD)。シークレットをハードコードしない。リポジトリにAGENTS.mdがある場合はそれに従う。
