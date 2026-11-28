![Harmony for Europa Universalis IV](docs/images/banner.jpg)

[![CI](https://github.com/KisinTheFlame/eu4-harmony/actions/workflows/ci.yml/badge.svg?branch=master)](https://github.com/KisinTheFlame/eu4-harmony/actions/workflows/ci.yml)

[English](README.md) | [简体中文](README.zh-CN.md) | 日本語 | [한국어](README.ko.md)

**Europa Universalis IV を内部からより快適に。** くっきりとした日本語・中国語・韓国語フォント表示、メインメニュー復帰時の再起動不要、後半の動作高速化、エンジン不具合の修正、そして高機能な Mod 拡張基盤を、ゲーム本来のルールを変えずに提供します。

> 目次: [プレイヤー向け](#プレイヤー向け) · [Mod 制作者向け](#mod-制作者向け) · [コントリビューター向け](#コントリビューター向け)

---

## プレイヤー向け

### バニラ版 EU4 との比較

| | バニラ版 | Harmony 導入後 |
| --- | :-: | :-: |
| システムフォントとライブプレビューによる鮮明な日本語・中国語・韓国語表示（旧パッチ・ビットマップフォント Mod 不要） | ✗ | ✓ |
| 再起動なしで数秒でメインメニューへ復帰 | ✗ | ✓ |
| アイアンマン進行中でも別セーブを直接ロード可能 | ✗ | ✓ |
| 速度 5 でも滑らかなマップ移動・ズームと、後半カクつきの軽減 | ✗ | ✓ |
| セーブ時の画面停止時間を約 3 分の 1 に短縮 | ✗ | ✓ |
| 起動・ロード・後半の日次／月次進行を高速化 | ✗ | ✓ |
| 後継者名の重複・復帰時メモリリーク・宗教改革中心のクラッシュを修正 | ✗ | ✓ |
| BGM 音量の均一化と自然な音量スライダーカーブ | ✗ | ✓ |
| オプションで実績条件を緩和（非アイアンマンや Mod 導入時） | ✗ | ✓ |
| ゲーム標準風 UI と自動化機能を備えた Mod を透明な権限表示付きで実行 | ✗ | ✓ |
| **ゲームルール・データは一切変更なし** | ✓ | ✓ |

ゲームの日本語訳テキストはこれまで通り日本語化 Mod が提供します。旧マルチバイトパッチ向けの翻訳 Mod やそのセーブデータもそのまま利用できます。Harmony は **Steam Windows 版 EU4 1.37.5** のシングルプレイ専用です（マルチプレイ対応は開発中）。

### インストール

**重要：サブスクライブしただけでは有効になりません。** 初回のみ 1 ファイル（`version.dll`）をゲームフォルダへコピーしてください：

1. Steam ワークショップで **Harmony** をサブスクライブします。
2. Steam で **Europa Universalis IV** を右クリック → **管理 → ローカルファイルを閲覧** でゲームフォルダを開きます（開いたままにします）。
3. Paradox ランチャー → **Mod ライブラリ** → **Harmony** → 右側の **… → フォルダで表示** をクリックします。（表示されない場合は Steam を再起動するか、EU4 と同じ Steam ライブラリ内の `steamapps\workshop\content\236850\<id>\` を開いてください。）
4. Harmony フォルダの `version.dll` をゲームフォルダの `eu4.exe` の隣へコピーします。（拡張子が非表示の場合は `version` と `eu4` と表示されます。旧パッチの `version.dll` が既にある場合は**置き換え**てください。元に戻せるようにしたい場合は先に退避してください。）

通常通り Paradox ランチャーから起動してください。Harmony 自体はプレイセットに追加不要です（Harmony 対応 Mod は通常通りプレイセットで有効化します）。有効化されるとメインメニューとゲーム内メニュー右下に **Harmony v…**・**Mod一覧**・**設定** が表示されます。

更新は Steam が自動で行います。新しい `version.dll` が含まれる更新では、起動時に自動でコピーを更新してゲームを再起動します。

#### 手動インストール（GitHub Releases）

[GitHub Releases](https://github.com/KisinTheFlame/eu4-harmony/releases) から `eu4-harmony-<version>-windows-x64.zip` をダウンロードし、ゲームを閉じた状態で `eu4.exe` と同じフォルダへ展開することもできます。ローカル版はワークショップ版より優先されるため自動更新されません。自動更新へ戻すにはゲームフォルダ内 `eu4-harmony\` の `eu4_harmony_core.dll`・`eu4_harmony_crash_dumps.dll`・`fonts\` を削除してください。一般プレイヤーは CLI アーカイブ（`eu4-harmony-cli-*.zip`）を導入する必要はありません。

#### アンインストール

Harmony のサブスクライブを解除し、ゲームフォルダの `version.dll` を削除してください（退避していた旧ファイルがあれば戻します）。**Harmony 設定 → 情報** で Windows クラッシュダンプを有効にしていた場合は、先に同画面の **無効にする** ボタンを押してください。

### よくある質問（FAQ）

- **サブスクライブしたのにメインメニューに Harmony が表示されません。**
  サブスクライブだけでは有効になりません。手順 4 の通り `version.dll` が `eu4.exe` の隣にコピーされているか、また Steam Windows 版 1.37.5 であるか確認してください。
- **セキュリティソフトが `version.dll` や `eu4_harmony_core.dll` を警告します。安全ですか？**
  Harmony は MIT ライセンスで全ソースコードを公開しています。ゲームプロセス内へ読み込む仕組み上、まれにヒューリスティック誤検知が起きることがあります。コードの確認、EU4 フォルダの除外設定、または自己ビルドが可能です。
- **旧マルチバイトパッチやフォント Mod から移行するには？**
  旧 `version.dll` を Harmony のファイルに置き換え、旧ビットマップフォント Mod を無効化してください（翻訳テキスト Mod は有効のまま）。既存セーブもそのまま読み込めます。
- **マルチプレイに対応していますか？**
  マルチプレイ対応は現在開発中です。現状はシングルプレイ専用で、メインメニューのマルチプレイボタンは無効化されます。マルチプレイを遊ぶ際はサブスクライブを解除するか、ゲームフォルダの `version.dll` を一時的に外へ移動してください。
- **後で Harmony を外してもセーブデータは使えますか？**
  Harmony 本体のみであればセーブ構造は壊れませんが、導入中に手動入力した日本語名などは UTF-8 で保存されるため外すと文字化けします。独自データをセーブに書き込む Harmony Mod を使用した場合、Harmony なしでの読み込みは保証されません。
- **音が小さくなった／非アイアンマン実績が既存セーブに効かないのはなぜ？**
  音量スライダーを聴感に沿った二乗カーブへ変更したため、100 未満の値は以前より静かに聞こえます（一度調整し直してください）。非アイアンマン実績は新規キャンペーン開始前に有効化する必要があります（[詳細](docs/game/countries/achievements.md#each-achievements-script-conditions)）。

### トラブルシューティング

メインメニューまたはゲーム内メニューで **Harmony 設定 → 情報** を開き、**トラブルシューティング用ファイルをデスクトップに保存** をクリックすると、ログ・直近のクラッシュ情報（大きいダンプは自動軽量化）・設定・プレイセット（セーブなし、24 MiB 未満）をまとめた zip が作成されます。情報ページの GitHub Issues リンクから、その zip を添付して報告してください。

ログは `Documents\Paradox Interactive\Europa Universalis IV\eu4-harmony\logs\` に保存されます。詳細は [docs/logs.md](docs/logs.md) を参照してください。

---

## Mod 制作者向け

### Harmony 対応 Mod の作成

Harmony 対応 Mod は通常の Paradox Mod に `eu4-harmony/` フォルダを追加した構成です。Lua 拡張はメモリアドレスを一切触れず、安全なケイパビリティ API を通じてゲームを読み書きします：

```lua
-- eu4-harmony/lua/main.lua：eu4-harmony/ui/ で宣言したボタンを
-- 自軍選択中のみ軍隊パネルに表示する例
return {
  on_frame = function(game)
    game.ui.button.visible = game.army.selected() ~= nil
  end,

  on_button = function(game)
    log("army " .. tostring(game.army.selected()) .. " clicked")
  end,
}
```

[GitHub Releases](https://github.com/KisinTheFlame/eu4-harmony/releases) から `eu4-harmony.exe` をダウンロードして使用します：

```powershell
eu4-harmony new my-mod --name "My Mod"   # UI・ローカライズ・型定義付きの雛形を生成
cd my-mod
eu4-harmony check                         # フレームワークと同様に全ファイルを検証
eu4-harmony test                          # 擬似ゲーム環境で eu4-harmony\tests\*.lua を実行
eu4-harmony link                          # ランチャーへ登録しプレイセットで有効化
eu4-harmony reload --watch                # ゲーム起動中に変更を検知してホットリロード
eu4-harmony types                         # CLI 更新後に LuaLS 型定義を再生成
eu4-harmony package                       # ワークショップ提出用のクリーンなフォルダを作成
```

外部アクセス権限（`clock`・`files`・`network`）は `manifest.toml` で宣言し、プレイヤーへ **Harmony Mod** ウィンドウで明示されます。

詳細ドキュメント：

- `eu4-harmony wiki`：全ケイパビリティの仕様、ハンドラ／グローバル関数（`wiki lua`）、マニフェスト仕様（`wiki manifest`）。ゲーム本体不要。
- `eu4-harmony skills`：作業別の実践ガイド（`skills mod`・`skills ui`・`skills testing` 等）。
- [cli/skills/mod.md](cli/skills/mod.md)：拡張機能の動作、エラー処理、時間・メモリ制限、セーブデータ。

### リモート制御と自動化

プログラムや AI から `eu4-harmony game <capability> [<arguments>]` でゲームを操作し JSON で結果を受け取れます：

```powershell
eu4-harmony start --hidden                 # バックグラウンド起動（Steam 起動が必要）
eu4-harmony game menu start FRA            # フランスで新規ゲーム開始
eu4-harmony game time advance 30           # 30 日進行（イベント発生時は途中で停止）
eu4-harmony game question list             # 応答待ちのイベント一覧を取得
eu4-harmony game army move 545460846646 185
eu4-harmony game screenshot --out shot.png
eu4-harmony game menu return               # メインメニューへ復帰
```

観戦モード（国タグなしで `menu start`）では `session view FRA` / `session leave_view` で視点を切り替えられます（[docs/game/countries/countries.md](docs/game/countries/countries.md)）。リモート制御はローカルの実行ユーザー専用です。詳細は `eu4-harmony wiki remote` を参照してください。

---

## コントリビューター向け

### ドキュメント

[docs/README.md](docs/README.md) がゲーム解析メモ・開発ツール・ADR・検証記録の入り口です。用語集は [GLOSSARY.md](GLOSSARY.md)、未完了タスクは [docs/plans/README.md](docs/plans/README.md) を参照してください。

### ソースからのビルド

Windows 10 (1903+) x64、[rustup](https://rustup.rs/)（[rust-toolchain.toml](rust-toolchain.toml) を自動参照）、Visual Studio C++ ビルドツールが必要です。EU4 1.37.5 本体はフレームワーク実行・オフライン検証・テストゲーム実行時のみ必要です。

```powershell
cargo build --release
cargo test
cargo xtask offline-check          # EU4 1.37.5 が必要（Steam または EU4_GAME_DIR）
cargo xtask release --no-workshop  # SteamCMD なしで GitHub リリース用 ZIP を生成
```

隔離テストゲームやシナリオ検証（Steam 起動・EU4 1.37.5 全 DLC 環境）：

```powershell
cargo xtask install                # ゲーム終了後に初回のみ実行
cargo xtask saves fetch            # 共有テストセーブの取得と検証（初回のみ）
cargo xtask scenario sweep --owner <name>
```

開発・リリース・テストゲーム手順は [docs/dev-tools.md](docs/dev-tools.md)、規約とコミット手順は [AGENTS.md](AGENTS.md) をご確認ください。変更は Pull Request を通じて `master` へ取り込まれます。

### ライセンス

[MIT](LICENSE)。本リポジトリのコードとドキュメントのみが対象であり、ゲーム本体は含まれません。Mod 制作には Paradox の Mod 利用規約が適用されます。
