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

## 功能（M2 進行中）

- **表格**：
  - **顯示**：格線、合併儲存格、框線、填色，以及表格樣式（標題列、交錯底色）。檔案沒有定義預設樣式時，用內建的「中度樣式 2 - 輔色 1」。
  - **編輯**：點儲存格直接輸入，Tab／⇧Tab 換格（最後一格按 Tab 會新增一列）；列高會隨文字自動長高。
  - **調整**：拖曳內部格線調整欄寬、列高；拖曳外框移動表格。「表格版面配置」分頁可以插入或刪除列、欄；「插入 > 表格」用格子選擇器選大小。
  - **存檔**：只改文字或欄寬時，只改寫變動的儲存格；插入或刪除過列欄時，整個表格重新產生。
- **群組**：
  - **選取**：點一下選取整組；再點一下或雙擊進入群組，編輯個別圖形。
  - **編輯**：整組移動或縮放；⌘⌥G 組成群組，⌘⌥⇧G 取消群組。
  - **存檔**：只移動或縮放整組時，只改外層群組的位置，巢狀結構完整保留；成員改過時重建群組。
- **圖形**：
  - **預設圖形**：內建 ECMA-376 `presetShapeDefinitions.xml` 的公式引擎，支援全部 180 多種預設圖形，包含連接線、弧形、調整值和文字區域。
  - **填色**：漸層（線性、放射），以及佈景主題的填色樣式（`fillRef`）。
  - **線條**：虛線樣式和箭頭（三角形、箭號、菱形、圓形）。
- **SmartArt**：用檔案裡預先繪製的圖形顯示，存檔原樣保留。
- **文字**：
  - **編號清單**：1. 2. 3.、I. II.、a)、①②③、一. 二. 等，提供編號格式選單。
  - **項目符號**：可以有自己的字型、顏色、大小；Wingdings 等符號字型會換成對應的 Unicode 字元。
  - **超連結**：存檔保留，⌘K 新增或移除；連結文字以佈景主題的超連結色加底線顯示。
- **其他**：
  - 圖片裁切，以及依形狀外框遮罩。
  - 簡報裡有 emoji 時，才載入 Apple Color Emoji 字型；Apple Symbols 和 Zapf Dingbats 作為符號的備用字型。

## 功能（M1）

- **開啟與儲存 PPTX**：沒改過的投影片原樣保留；改過的投影片只重寫有變動的圖形，而且只寫和母片繼承不同的屬性。圖表、SmartArt 等尚不支援的物件會原樣保留。
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
| `crates/ppt-model` | 文件模型（EMU 座標、文字段落與樣式、表格與表格樣式） |
| `crates/ppt-ooxml` | PPTX 匯入與儲存、內建範本：佈景主題、佔位符繼承、文字樣式鏈、群組、圖片、表格 |
| `crates/ppt-geometry` | 預設圖形公式引擎（`presets.xml` 取自 Apache POI，Apache 2.0）、自訂幾何、文字區域、座標轉換 |
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
