# claude-usage

Claude Code の 5 時間ローリングウィンドウ使用量を表示する Tauri デスクトップアプリ。

`~/.claude/projects/**/*.jsonl` をスキャンして、 現在進行中の 5h ウィンドウについて以下を表示する:

- 経過率 (プログレスバー) + リセットまでの残時間
- 累計コスト (USD)
- メッセージ数
- 入力 / 出力 / キャッシュトークン数
- モデル別内訳

2 秒ごとに自動更新。

## 動作環境

- **Windows 10/11** (本リポジトリの想定ターゲット。 macOS / Linux でも Tauri 標準依存を満たせば動く)
- Rust 1.96 以降
- Node.js 18 以降 + pnpm

## Windows セットアップ

1. **Rust toolchain**: <https://rustup.rs/> から `rustup-init.exe` を取得して実行。 デフォルト (stable, x86_64-pc-windows-msvc) で OK。
2. **Microsoft C++ Build Tools**: Visual Studio Installer から "Desktop development with C++" を入れる ([Tauri prerequisites](https://tauri.app/start/prerequisites/#microsoft-c-build-tools) 参照)。
3. **WebView2**: Windows 11 は標準搭載。 Windows 10 の場合は [Evergreen Bootstrapper](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) を入れる。
4. **Node.js + pnpm**: `winget install OpenJS.NodeJS` の後、 `corepack enable pnpm` か `npm i -g pnpm`。

## 開発

```powershell
git clone <this-repo>
cd claude-usage
pnpm install
pnpm tauri dev
```

`pnpm tauri dev` で開発モード起動。 frontend の編集は HMR で即反映、 Rust 側の編集は自動で再 build される。

## リリースビルド

```powershell
pnpm tauri build
```

`src-tauri/target/release/bundle/` に MSI / NSIS インストーラーが生成される。 単体実行用の exe だけ欲しい場合は `src-tauri/target/release/claude-usage.exe`。

## プロジェクト構成

```text
claude-usage/
├── index.html                  # frontend エントリ
├── src/
│   ├── main.ts                 # invoke('get_usage_summary') を 2 秒間隔で叩く
│   └── styles.css              # ダーク基調シンプル
├── src-tauri/
│   ├── Cargo.toml              # Rust 依存定義
│   ├── tauri.conf.json         # window サイズ・identifier・bundle 設定
│   ├── capabilities/default.json
│   └── src/
│       ├── main.rs             # エントリ (lib.rs::run を呼ぶだけ)
│       ├── lib.rs              # Tauri builder + #[tauri::command] 配線
│       ├── paths.rs            # ~/.claude/projects を解決
│       ├── usage.rs            # jsonl スキャナ + パース
│       ├── pricing.rs          # モデル別単価テーブル
│       └── aggregator.rs       # 5h ウィンドウ集計 + コスト計算
└── package.json
```

## 読み合わせ用の地図 (Rust 初学者向け)

ファイルを以下の順に追うと、 Rust + Tauri の構造が掴みやすい。

1. **`src-tauri/src/main.rs`** — エントリポイント。 `claude_usage_lib::run()` を呼ぶだけ。 Windows で release ビルド時にコンソール窓を出さない `#![cfg_attr]` がある。
2. **`src-tauri/src/lib.rs`** — `mod` 宣言で module ツリーを定義し、 `#[tauri::command] fn get_usage_summary` で frontend から呼べる関数を 1 つだけ公開している。 `tauri::Builder::default().invoke_handler(...)` で配線。
3. **`src-tauri/src/paths.rs`** — クロスプラットフォームに `~/.claude/projects` を返す。 `dirs::home_dir()` が `Option<PathBuf>` を返すので、 そのまま伝播。
4. **`src-tauri/src/usage.rs`** — `walkdir` で jsonl を全部見て、 `serde_json::from_str` で各行をパース、 `assistant` 型かつ usage 持ちだけ `UsageEntry` に詰める。 `#[serde(rename = "type")]` で予約語回避、 `#[serde(default)]` でフィールド欠落許容、 `Option<T>` でフィールド有無を表現するのが Rust + serde の定番。
5. **`src-tauri/src/aggregator.rs`** — 「現在時刻から 5h 以内」 でフィルタして最古エントリを window 起点と見なす。 model 別に集計してコスト計算。 `BTreeMap` を使うと `String` キーで自動ソートされる。
6. **`src-tauri/src/pricing.rs`** — model prefix で分岐する単純なテーブル。 公式単価が変わったらここを直す。
7. **`src/main.ts`** — `invoke<UsageSummary>("get_usage_summary")` で Rust 側を呼んで、 DOM 更新するだけ。 型は Rust 側 `Serialize` 派生のフィールド名そのまま。

### よく弄りそうな場所

| やりたいこと | 弄るファイル |
|---|---|
| 更新間隔を変える | `src/main.ts` の `REFRESH_MS` |
| 表示項目を増やす | `index.html` + `src/main.ts` の `refresh()` |
| ウィンドウ時間を 5h 以外にする | `src-tauri/src/aggregator.rs` の `WINDOW_HOURS` |
| 単価を最新化する | `src-tauri/src/pricing.rs` |
| 新モデル追加 | `src-tauri/src/pricing.rs` の `lookup` に分岐追加 |
| ウィンドウサイズ・タイトル | `src-tauri/tauri.conf.json` |

## 注意

- 単価は実装時点 (2026-06) の Anthropic 公式公開価格を手書きしたもの。 改定があれば `pricing.rs` を直すこと。
- 「5h ウィンドウの最初のメッセージから 5h」 という Anthropic 公式仕様の解釈に基づく。 厳密な reset タイミングは保証しない。
- `<synthetic>` モデルのエントリ (Anthropic 側の合成メッセージ、 非課金) は集計対象外。
