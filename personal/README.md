# personal/ — 個人(マシン)側のグローバル設定

リポジトリ単位ではなく、個人のメモリ(グローバル設定)に置くべき指示。新しいマシンをセットアップするとき、または内容を更新したときに各エージェントへ反映する。

## 内容

| ファイル | 反映先 |
| --- | --- |
| `global-instructions.md` | `~/.claude/CLAUDE.md`(Claude Code)、`~/.codex/AGENTS.md`(Codex) |
| `stop-hook-git-check.sh` | `~/.claude/stop-hook-git-check.sh`(Claude CodeのStopフック本体) |
| `claude-user-settings-snippet.json` | `~/.claude/settings.json`へ手動でマージ(Stopフックの登録) |
| `copilot-personal-instructions.md` | GitHub Copilotの個人カスタム指示(手動で貼り付け) |

## 反映方法

- Linux / macOS / WSL2: `./install.sh`
- Windows(PowerShell): `./install.ps1`

既存ファイルの内容が異なる場合は、タイムスタンプ付きの`.bak`にバックアップしてから上書きする。

スクリプトで反映できない手動手順:

- Stopフックの登録: `claude-user-settings-snippet.json`の内容を`~/.claude/settings.json`にマージする(既存設定を壊さないよう自動マージはしない)。
- Copilotの個人指示: `copilot-personal-instructions.md`の手順に従って貼り付ける。

## Stopフック(git状態チェック)について

Claude Codeが応答を終える前に、未コミットの変更・未追跡ファイル・署名に問題のあるコミット(未署名・署名破損・committerメールが期待値と不一致。GitHubでは通常Unverified表示になる)・未プッシュのコミットを検知してエージェントに知らせる。

- GitHub生成のコミット(committerメールが`noreply@github.com`のマージ/squashコミット)はGitHubのweb-flow鍵で署名されておりVerified表示になるため、committerメール不一致の検査からは除外している(未署名・署名破損はcommitterメールに関係なく検知する)。
- 署名の検査は`commit.gpgsign`が有効な環境(Claude Code on the Web等)でのみ動く。ローカルで自分の鍵で署名する場合はスクリプト冒頭の`expected_email`を調整する。
- Claude Code on the Webでは同種のフックが環境側で配置されることがある。内容が異なる場合はインストール時にバックアップされる。
- Windowsネイティブ環境では[Microsoft Core Utils](https://learn.microsoft.com/ja-jp/windows/core-utils/overview)を利用する前提とする([microsoft/coreutils](https://github.com/microsoft/coreutils): uutils版coreutils・findutils・grepのMicrosoftビルド。`winget install Microsoft.Coreutils`)。ただしbash・awk・sedは含まれないため、フック本体(bashスクリプト)の実行にはClaude CodeのBashツールと同じくGit Bashを用いる。`$HOME`が解決されない場合はsettings.jsonのcommandをフルパス指定に調整する。

## 推奨gitグローバル設定

- `git config --global fetch.prune true`: マージで削除されたリモートブランチの追跡refを自動整理する。放置するとStopフックが「unpushed commits」を誤検知する原因になる。

## リポジトリのAGENTS.mdとの関係

- 内容が一部重複するのは意図的。`AGENTS.md`がないリポジトリでも共通スタイルを効かせるため。
- リポジトリ側の`AGENTS.md`の指示が常に優先される。個人設定にはプロジェクトに依存しない好みだけを書く。

## 実運用リポジトリでは

テンプレートから作成した実運用リポジトリでは、このフォルダは削除する(各マシンへの反映が済んでいる前提)。
