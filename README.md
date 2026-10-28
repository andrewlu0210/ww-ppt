# ww-ppt

用 Rust + Tauri v2 開發的 macOS 簡報 App，目標是做到和 Microsoft PowerPoint 一樣的操作體驗。架構和階段規劃見 [ppt-plan.md](ppt-plan.md)。

目前進度：**M2 PPTX 相容**進行中（M0 技術驗證、M1 核心編輯器已完成）。

## 環境需求

- macOS 26 以上（WKWebView 需要支援 WebGPU）
- rustup 管理的 Rust stable，加上 `wasm32-unknown-unknown` target
  - Homebrew 的 rustc 沒有 wasm target，腳本會優先使用 `~/.cargo/bin`
- `wasm-bindgen-cli`，版本必須和 `crates/ppt-wasm/Cargo.toml` 一致：`cargo install wasm-bindgen-cli --version 0.2.129`
- Node.js 24 以上

## 執行

```sh
npm install
npm run wasm                       # 編譯編輯器核心 → ui/src/wasm
npm run tauri dev                  # 啟動 App
npm run tauri dev -- -- -- /絕對路徑/deck.pptx   # 啟動時直接開啟檔案
```

測試：

```sh
cargo test --workspace
# 範例簡報 samples/m0-demo.pptx 可以重新產生：
uv run --no-project --with python-pptx python scripts/make_sample_pptx.py samples/m0-demo.pptx
```

## 視覺回歸測試

`samples/vr/` 裡每份 `<名稱>.pptx` 搭配一份從 PowerPoint 匯出的 `<名稱>.pdf` 當基準。工具會逐頁渲染並和 PDF 比對，另外會把簡報存檔再開啟，確認存檔後畫面不變。需要 poppler 的 `pdftoppm`（`brew install poppler`）。

```sh
cargo run -p ppt-vr --release                 # 和 samples/vr/baseline.json 比對，有頁面退步就失敗
cargo run -p ppt-vr --release -- --update     # 修正後，把目前分數寫成新基準
cargo run -p ppt-vr --release -- --only test  # 只跑一份
```

- **分數**：不一致像素佔整頁的百分比。容許 1px 位移和反鋸齒差異，所以剩下的差異主要來自字型不同，例如微軟雅黑被替換成蘋方。
- **結果**：輸出到 `target/vr/<名稱>/`。`page-NN.png` 由左到右是基準、本程式渲染、差異（紅色）；`report.md` 是分數表。
- **私人樣本**：`samples/vr/test.*` 是真實工作簡報，已列在 `.gitignore`，不要提交。

## 功能（M1）

- **開啟與儲存 PPTX**：沒改過的投影片原樣保留；改過的投影片只重寫有變動的圖形，而且只寫和母片繼承不同的屬性。表格、圖表、SmartArt 等尚不支援的物件會原樣保留。
- **新增簡報**：內建 16:9 Office 佈景主題範本，含 6 種版面配置。
- **富文字**：逐字設定字型、大小、粗斜體、底線、顏色；段落對齊、項目符號、清單階層（Tab／⇧Tab）、行距。支援注音等 IME 組字；文字框內 ⌘Z 會先復原打字。
- **圖形**：
  - 新增圖形、圖片、文字方塊；填滿、外框顏色和寬度。
  - 多選（⇧ 點按）、框選、智慧參考線（按住 ⌥ 暫時關閉）。
  - 複製貼上、⌘D 複製、圖層順序、對齊。
- **投影片**：依版面配置新增、複製、刪除、拖曳排序；左側為即時縮圖；空白佔位符會顯示提示文字。
- **其他**：未儲存提醒（開新檔、開啟、關閉視窗時）。

## 結構

| 路徑 | 內容 |
|---|---|
| `crates/ppt-model` | 文件模型（EMU 座標、文字段落與樣式） |
| `crates/ppt-ooxml` | PPTX 匯入與儲存、內建範本：佈景主題、佔位符繼承、文字樣式鏈、群組、圖片 |
| `crates/ppt-geometry` | 預設圖形與自訂幾何的外框、座標轉換 |
| `crates/ppt-text` | 字型註冊、字型替代；整個文字框一個 parley 排版，逐行套用段落縮排與對齊 |
| `crates/ppt-render` | 用 vello 繪製投影片、選取框、文字游標 |
| `crates/ppt-editor` | 選取、多選、參考線、縮放、旋轉、富文字編輯（`richedit.rs`）、IME、格式、投影片操作、復原 |
| `crates/ppt-wasm` | 給前端呼叫的 WASM 介面，包含 WebGPU 繪製流程 |
| `crates/ppt-fonts` | 原生字型服務：用 fontique 找系統字型檔，加上 Office 字型替代表（App 和測試共用） |
| `crates/ppt-vr` | 視覺回歸測試：用原生 wgpu 離線渲染投影片，和 PowerPoint 匯出的 PDF 比對 |
| `src-tauri` | 原生後端：讀檔、字型服務、選單 |
| `ui` | React 外框：Ribbon、投影片清單、畫布、IME 輸入橋接 |

## 開發用自動化

設定 `WW_PPT_AUTOMATION=<資料夾>` 啟動後，App 會讀取 `<資料夾>/commands.txt`，每行一個指令，例如 `click 400 300 2`、`drag x1 y1 x2 y2`、`down`/`move`/`up`、`text 你好`、`compose ㄋㄧˇ`、`commit 你`、`key Escape`、`run {"kind":"newSlide"}`、`save 路徑`、`shot 名稱`、`thumbs 名稱`。執行結果寫到 `log.txt`，畫布截圖存成 `名稱.png`。實作見 `ui/src/lib/automation.ts`。

存檔的開發工具：`cargo run -p ppt-ooxml --example resave -- in.pptx out.pptx [--edit]`（`in.pptx` 可用 `template` 代表內建範本）。
