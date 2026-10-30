# Rust + Tauri v2 macOS 簡報 App 技術架構規劃

## 一、核心架構決策

要做到「跟 PowerPoint 一樣」，最難的有三件事：**排版精準度**、**操作延遲**、**PPTX 相容性**。下面幾個決策都是為了處理這三件事。

| 決策 | 建議 | 理由 |
|---|---|---|
| 投影片畫布怎麼畫 | **自己寫 Rust 渲染引擎，編譯成 WASM，在 WebView 裡用 WebGPU 畫**，不用 DOM 排版 | 用 DOM/CSS 排版，文字換行和字距沒辦法對齊 PowerPoint。編輯畫面、縮圖、匯出 PDF、放映這四個地方必須用同一套排版結果 |
| 文件狀態放哪裡 | **放在前端的 WASM 裡**，Rust 後端只負責 I/O 和系統功能 | Tauri 的 IPC 每次來回大約要 1–3ms。拖曳物件要跑 120Hz，不能每一幀都跟後端溝通 |
| UI 外框（Ribbon、側欄） | React + TypeScript | 生態系成熟，複雜面板做起來最快 |
| 共用程式碼 | Cargo workspace，核心 crate 同時編譯成 native 和 wasm32 | 解析、排版、匯出邏輯只需要寫一份 |

## 二、整體架構圖

```
┌──────────────────────────── macOS App (Tauri v2) ────────────────────────────┐
│                                                                              │
│  ┌─────────────────────── WebView (WKWebView) ───────────────────────────┐   │
│  │                                                                       │   │
│  │  ┌─────────── UI 外框 (React + TS) ───────────┐                       │   │
│  │  │ Ribbon / 縮圖窗格 / 備忘稿 / 格式面板 /     │                       │   │
│  │  │ 對話框 / 右鍵選單 / 狀態列                  │                       │   │
│  │  └───────────────┬────────────────────────────┘                       │   │
│  │                  │ wasm-bindgen（同步呼叫，零 IPC）                    │   │
│  │  ┌───────────────▼──────────── Editor Core (Rust → WASM) ───────────┐ │   │
│  │  │ Document Model │ Command/Undo │ Selection │ Hit-test │ Snapping  │ │   │
│  │  │ Text Layout (parley/rustybuzz) │ Shape Geometry │ Display List   │ │   │
│  │  │ Renderer (vello on WebGPU，退回方案 tiny-skia → Canvas2D)        │ │   │
│  │  └───────────────┬──────────────────────────────────────────────────┘ │   │
│  │                  │ 隱藏的 <textarea>：接 IME 組字（注音/拼音）       │   │
│  └──────────────────┼────────────────────────────────────────────────────┘   │
│                     │ Tauri IPC（Commands + Channels，大檔走 binary）        │
│  ┌──────────────────▼───────────── Rust Backend (native) ─────────────────┐  │
│  │ 檔案 I/O / Autosave / 復原 │ PPTX 匯入匯出（共用 core crate）          │  │
│  │ 字型服務（Core Text 列舉、讀取字型檔、替代對照表）                     │  │
│  │ PDF / 圖片匯出（krilla）│ 媒體轉檔（EMF/WMF/HEIC/影片）               │  │
│  │ 放映模式多視窗 / 簡報者檢視 │ 原生選單、剪貼簿（NSPasteboard）         │  │
│  │ 最近檔案、檔案關聯（.pptx UTI）、自動更新                              │  │
│  └────────────────────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────────────────┘
```

## 三、Cargo Workspace 結構

```
ww-ppt/
├── Cargo.toml                 # workspace
├── crates/
│   ├── ppt-model/             # Document model (no_std-friendly, serde)
│   ├── ppt-ooxml/             # PPTX parse/serialize (quick-xml + zip)
│   ├── ppt-geometry/          # 187 preset shapes + custom geometry formula engine
│   ├── ppt-text/              # Rich text layout (parley / rustybuzz / swash)
│   ├── ppt-render/            # Display list + vello / tiny-skia backends
│   ├── ppt-editor/            # Commands, undo/redo, selection, hit-test, snapping
│   ├── ppt-anim/              # Transitions & animation timeline
│   ├── ppt-export/            # PDF (krilla), PNG, video (later)
│   └── ppt-wasm/              # wasm-bindgen facade for the frontend
├── src-tauri/                 # Tauri v2 backend (commands, plugins, windows)
└── ui/                        # React + TS + Vite
    ├── ribbon/  panes/  dialogs/  canvas-host/  ime-bridge/
```

## 四、各模組技術細節

### 1. Document Model（`ppt-model`）
- **跟 OOXML 的繼承鏈對齊**：Presentation → SlideMaster → SlideLayout → Slide，佔位符（placeholder）屬性一層層繼承，加上 Theme（色彩、字型、效果）。
- 物件用 **Arena + 穩定 ID**（`slotmap`），不用指標樹，方便做 undo、序列化和未來的協作。
- **保留看不懂的 XML**：不支援的元素原樣存成 `RawXml`，存檔時寫回去。這是不破壞使用者原始檔案的關鍵。
- 如果有協作的打算，建議從第一天就讓所有修改走 **operation**（之後可以接 Loro 或 yrs CRDT）。

### 2. 文字排版（`ppt-text`）— 最難、最花時間
- 用 `parley`（Linebender）做段落排版，`rustybuzz` 做 shaping，`swash` 取字形輪廓。
- 要支援的功能：自動調整（縮小字型以符合文字框）、欄位、行距（倍數/固定值）、段前段後、項目符號與編號、Tab stop、直排文字、CJK 禁則、中英混排字型 fallback（`latin` / `ea` / `cs` 三種 script 字型）。
- **字型替代對照表**：Mac 預設沒有 Calibri、Cambria，要對應到度量相容的 Carlito、Caladea，否則換行位置會跑掉。中文對應：微軟正黑體 → 蘋方-繁。
- 字型檔由 native 後端用 Core Text 列舉、按需讀取後送進 WASM，並加上快取。

### 3. 圖形與渲染（`ppt-geometry` / `ppt-render`）
- 實作 ECMA-376 的 `presetShapeDefinitions.xml`（約 187 種預設圖形），需要一個 guide 公式的求值器。
- 渲染流程：Model → Layout → **Display List**（平台無關）→ 後端。
  - 編輯畫面：vello + WebGPU（WKWebView 已支援）
  - 縮圖：低解析度離屏渲染，放 LRU 快取
  - PDF：同一份 Display List 交給 krilla
- 效果：陰影、光暈、柔邊、漸層、圖樣填滿、圖片裁切、透明度、3D（先做簡化版）。

### 4. 編輯互動（`ppt-editor`）
- Command pattern 加 undo/redo stack，連續拖曳合併成一筆。
- 命中判定用 R-tree（`rstar`），旋轉後的物件用 OBB。
- **智慧參考線與對齊吸附**：PowerPoint 的手感有很大一部分來自這裡，值得多花時間調。
- 控制點：縮放（8 點）、旋轉、黃色調整點（adjust handle）、連接線端點、編輯頂點。
- **IME 是必做項目**：畫布上沒有原生文字輸入，要在游標位置放一個隱藏的 `<textarea>` 接 `compositionstart/update/end`，組字中的文字畫在畫布上並加底線（Figma 和 Google Docs 都是這樣做）。

### 5. UI 外框（React）
- Ribbon：常用、插入、繪圖、設計、轉場、動畫、投影片放映、校閱、檢視，加上依情境出現的「圖形格式」「表格設計」等分頁。
- 狀態管理用 Zustand，UI 只讀取 WASM core 提供的 snapshot 和 selection，不另外存一份文件。
- **macOS 原生感**：
  - `titleBarStyle: Overlay`，紅綠燈按鈕融入工具列
  - `window-vibrancy` crate 做側欄半透明效果
  - 原生選單列用 Tauri Menu API，快捷鍵遵循 ⌘ 慣例
  - 支援深色模式、觸控板捏合縮放與兩指平移

### 6. PPTX 相容性（`ppt-ooxml`）
- 用 `zip` 和 `quick-xml`（serde 或手寫 SAX 都可以，大檔用 streaming）。
- 依優先順序支援：
  - **P0**：文字、圖形、圖片、母片/版面配置/佈景主題、表格
  - **P1**：圖表（內嵌 xlsx 加 c:chart 渲染）、群組、連接線、轉場、動畫
  - **P2**：SmartArt（先用檔案裡的 fallback drawing）、OMML 方程式、影音、批註
- EMF/WMF 圖片需要另外轉成 SVG 或點陣圖，是常見的雷區。
- **測試**：蒐集上千份真實 PPTX，在 CI 做 round-trip 測試和視覺比對（用 PowerPoint 匯出的 PDF 當基準，算像素差）。

### 7. 投影片放映
- 用 Tauri 多視窗：外接螢幕開全螢幕放映視窗，主螢幕開簡報者檢視（目前頁、下一頁、備忘稿、計時器）。
- 兩個視窗用 Tauri event 同步，各自載入同一份 WASM 渲染器。
- 動畫時間軸引擎（`ppt-anim`）要對應 PowerPoint 的 timing tree：進入、強調、結束、移動路徑，以及觸發條件（按一下 / 與前動畫同時 / 接續前動畫）。

### 8. Tauri v2 後端
- 會用到的 plugins：`fs`、`dialog`、`clipboard-manager`、`updater`、`window-state`、`single-instance`、`deep-link`。
- 大型二進位資料（圖片、字型、整份檔案）用 `tauri::ipc::Response` 和 `Channel` 傳原始位元組，避免 JSON 加 base64 的開銷。
- Autosave 加當機復原：定期把 operation log 寫到 `~/Library/Application Support`。
- 發佈：Developer ID 簽章、notarization、`.pptx` 的 UTI 檔案關聯。Quick Look 預覽需要另外寫一個 Swift app extension（之後再做）。

## 五、效能目標

| 項目 | 目標 |
|---|---|
| 拖曳 / 縮放物件 | 穩定 120fps（ProMotion） |
| 打字到畫面更新 | < 16ms |
| 開啟 100 頁、50MB 的 PPTX | < 2 秒可開始編輯（縮圖背景產生） |
| 冷啟動 | < 1 秒 |
| 記憶體 | 100 頁的檔案 < 500MB |

## 六、開發階段

1. **M0 技術驗證（4–6 週）**：WASM + vello 在 WKWebView 跑起來、一個文字框可輸入中文（含 IME）、能拖曳和縮放、讀進一份簡單的 PPTX 並正確顯示。**先驗證這一步再往下做。**
2. **M1 核心編輯器**：Document model、圖形與文字完整編輯、undo/redo、母片繼承、縮圖窗格、存檔。
3. **M2 PPTX 相容**：round-trip、表格、圖表、群組、視覺回歸測試建立起來。
4. **M3 放映**：轉場、動畫、簡報者檢視、多螢幕。
5. **M4 打磨**：Ribbon 全功能、匯出 PDF、尋找取代、拼字檢查（NSSpellChecker）、無障礙。
6. **M5 進階**：SmartArt 編輯、匯出影片。
   - **M5.1 SmartArt 編輯**：讀 `dgm:dataModel` 與版面定義；可改文字、新增/刪除節點、升降階層，改完重新排版；存檔時同時更新資料模型與畫好的圖形（`dsp:drawing`）。先支援清單、流程、階層等常見版面，其他版面維持唯讀。
   - **M5.2 匯出影片**：可選解析度、每頁停留秒數或照排練時間；逐格算出轉場與動畫畫面，由 Tauri 端用 AVFoundation 編成 H.264 `.mp4`，顯示進度、可取消。
   - ~~M5.3 Operation 層~~、~~M5.4 協作（CRDT）~~、~~M5.5 外掛~~：**決定不做**（2026-10-30）。Operation 層原本是協作與外掛的前置，兩者不做後一併取消；undo 維持快照式。

## 七、主要風險

1. **文字排版跟 PowerPoint 對不齊**：Office 有自己的換行和字距演算法，不可能 100% 一樣。需要用大量樣本反覆比對調整。
2. **無障礙**：畫布對 VoiceOver 不可見，要另外維護一棵隱藏的 ARIA DOM 樹。
3. **WebGPU 相容性**：舊版 macOS 要有 tiny-skia → Canvas2D 的 CPU 退回方案。
4. **工程量**：完整的 PowerPoint 是好幾年、多人的規模。建議 M0、M1 先把範圍收窄到「能好好編輯和放映常見簡報」。
