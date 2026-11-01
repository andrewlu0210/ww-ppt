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
- Autosave 加當機復原：有未儲存的變更時，停止編輯 3 秒後（最多每 30 秒一次）把整份簡報寫到 `~/Library/Application Support/com.ww.ppt/Recovery`；儲存、捨棄或關閉後刪除。啟動時若有已結束的程序留下的版本，詢問要不要復原。（已完成；不用 operation log，見 M5 的決定。）
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
     - 已完成：文字窗格（新增、刪除、升降階、移動）、排成一列與圓形的版面重新排版、文字放不下時自動縮小、尋找與取代。
     - 組織圖（`orgChart1`）：已完成新增、刪除、升降階與移動。畫面依樹狀重排：同層橫排、下一層可吊掛直排（兩種都從載入的圖形量出間距與縮排），連接線重畫，放不下時整體縮小；存檔時複製節點的 pres 點與 `presParOf` 關係。只有一個根節點，在根節點旁新增會加到它底下。其他組織圖版面（`hierarchy1` 等）仍只能改文字。
     - 金字塔（`pyramid1`，每層一個梯形）：已完成新增、刪除與移動。各層平分高度、寬度逐層加一階，斜邊角度（梯形 `adj`）依層數重算，頂層是三角形；存檔時把梯形的調整值寫回。其他金字塔版面（含多個圖形的）仍只能改文字。
     - 畫布上直接編輯：雙擊節點的方框，用一般的文字編輯器就地編輯（含輸入法），結束時把純文字寫回節點並重排，整段是一步復原。只限圖形裡的字剛好是該節點自己的文字；顯示子項目的清單方框、或 SmartArt 被旋轉／翻轉／縮放超過 2% 時，仍開文字窗格。編輯中的格式（粗體、顏色等）不會保留，重排後依版面的樣式。
     - 插入新的 SmartArt：「插入 > SmartArt」可選十七種版面：垂直項目符號清單（`vList2`）、水平項目符號清單（`hList1`）、基本流程（`process1`）、垂直流程（`process2`）、流程箭號（`process3`）、連續區塊流程（`hProcess9`）、基本人字形流程（`chevron1`）、封閉式人字形流程（`hChevron3`）、垂直人字形清單（`chevron2`）、基本循環（`cycle2`）、連續循環（`cycle3`）、區塊循環（`cycle5`）、分段循環（`cycle8`）、基本金字塔圖（`pyramid1`）、組織圖（`orgChart1`）、層次結構（`hierarchy1`）、水平層次結構（`hierarchy2`）。每種版面內嵌一組起手式（資料、版面定義、色彩、樣式、繪圖五個部件，取自既有簡報，文字清掉，見 `scripts/extract_smartart_starters.py` 與 `crates/ppt-ooxml/assets/smartart/`）；插入時把部件放進簡報套件、縮放到合適大小，再走一般的載入流程讀成 SmartArt，所以之後的編輯與存檔和開檔載入的完全相同，插入是一步復原。起手式是 3–4 個節點（組織圖是根加兩個部門各兩個小組），文字為「項目 1」等。
     - 拼字檢查：節點文字畫在自己的圖形裡時（例如清單、流程、組織圖、金字塔、循環的方框），拼錯的字會畫紅色波浪線，右鍵可更正，更正是一步復原；在畫布上編輯該節點時檢查的是即時文字。顯示在父節點圖形裡的子項目文字（如 `hList1` 的項目符號）、以及被旋轉或翻轉的 SmartArt 不檢查。
     - 層次結構（`hierarchy1`）：和組織圖同一套樹狀重排，節點可由多個圖形組成（方框加陰影，相對位置不變），連接線重畫；已可插入。
     - 水平層次結構（`hierarchy2`）：樹向右長，各層的欄位位置與寬度、子樹之間的間距從載入的圖形量出；連接線是從父節點右緣到子節點左緣的直線（旋轉過的線段，依兩端算出長度與角度）；已可插入。
     - 連續循環（`cycle3`）：節點沿圓等距重排，環繞整圈的圓弧箭頭只有一個、跟著第一個節點，首尾相對第一個節點的距離按節點間距的比例移動（比例從載入的圖形量出）；節點數改變使環超出框時整環（節點一起）向圓心收攏。已可插入。
     - 區塊循環（`cycle5`）：節點沿圓等距重排，節點之間各有一段帶箭頭的圓弧，依新位置重算：弧從上一個節點的方框離開圓處起、到下一個節點的方框進入圓處止，兩端各留載入時量出的距離；弧以貝茲曲線寫出。已可插入。
     - 分段循環（`cycle8`）：每個節點一片扇形加一條外緣的弧形箭頭。扇形等分整圈（起點、由中心外推的距離從載入的圖形量出）、箭頭首尾跟著扇形，文字放在扇形內可容納的最大直立矩形裡（以扇形角度搜尋，與載入圖形的差距量一次再帶過去）；存檔時重建整組 pres 點（扇形的點掛在文件上、靠 `presOf` 對到節點），文字框（`dsp:txXfrm`）依新的內縮寫回。已可插入。
     - 另外修了 SmartArt 文字框的一個舊問題：`dsp:txXfrm` 折進內縮後，又被預設幾何的文字矩形再縮一次（扇形最明顯，文字被擠成直排）；現在 `TextBody::exact_frame` 標示這種文字，改從圖形邊界量起。
     - 待補：其他版面的節點重排，各有獨有的排法。`radial2`、`StepDownProcess`、`lProcess2`（子項目是獨立圖形）都要各自的規則。每多一種可插入的版面，要先讓該版面的節點能重排，再從簡報取起手式。
   - **M5.2 匯出影片**：可選解析度、每頁停留秒數或照排練時間；逐格算出轉場與動畫畫面，由 Tauri 端用 AVFoundation 編成 H.264 `.mp4`，顯示進度、可取消。
     - 已完成：檔案選單與「投影片放映」分頁的「匯出為視訊」、4K／1080p／720p／480p、每張秒數與錄製時間、轉場與動畫依序播放、跳過隱藏投影片。
     - 待補：投影片裡的音訊與影片不會出現在匯出的視訊中；旁白錄製。
   - ~~M5.3 Operation 層~~、~~M5.4 協作（CRDT）~~、~~M5.5 外掛~~：**決定不做**（2026-10-30）。Operation 層原本是協作與外掛的前置，兩者不做後一併取消；undo 維持快照式。

## 七、主要風險

1. **文字排版跟 PowerPoint 對不齊**：Office 有自己的換行和字距演算法，不可能 100% 一樣。需要用大量樣本反覆比對調整。
2. **無障礙**：畫布對 VoiceOver 不可見，要另外維護一棵隱藏的 ARIA DOM 樹。
3. **WebGPU 相容性**：舊版 macOS 要有 tiny-skia → Canvas2D 的 CPU 退回方案。
4. **工程量**：完整的 PowerPoint 是好幾年、多人的規模。建議 M0、M1 先把範圍收窄到「能好好編輯和放映常見簡報」。

## 八、To Do

依建議的執行順序排列。現階段只給自己和朋友試用，先不申請 Apple Developer 帳號（未簽章 App 的開啟方式見 README「打包與試用」）。

### 1. 試用版打包與驗證

- [x] **打包**：`npm run tauri build` 產生 `.app`（24.6 MB）與 `.dmg`（11.2 MB）。整個 bundle 做 ad-hoc 簽章、最低系統 macOS 26；Info.plist 加 `LSHasLocalizedDisplayName`，中文系統的 Finder、選單列、程序名稱都顯示「旺-Point」。
- [x] **`.pptx` 檔案關聯**：補上 UTI `org.openxmlformats.presentationml.presentation`；處理 macOS 的開檔事件（`RunEvent::Opened`），App 未啟動或已開啟時都能從 Finder／「打開檔案的應用程式」開檔。系統預設仍是 PowerPoint，要預設用 Want-Point 需在 Finder「取得資訊」裡改。
- [x] **啟動時間量測（基準）**：打包版從 `open` 到編輯器就緒約 630–690 ms（2026-10-31，macOS 27.0.1，6 次）。新版本第一次啟動因系統安全掃描約 3.6 秒，之後恢復正常。未清除磁碟快取，重開機後的真正冷啟動尚未量。
- [ ] **朋友試用**：照 README 的步驟在另一台 Mac 開啟，確認流程可行。

### 2. 介面外觀（淺色、深色、跟隨系統）

範圍較小、主要改 CSS；先整理好顏色變數，做多語系時不必再回頭改同一批元件。

- [x] **顏色變數化**：把 CSS 裡寫死的顏色抽成 `:root` 變數，再補一組深色值（Ribbon、窗格、對話框、標題列等）。
- [x] **切換機制**：「檢視 > 外觀」三選一，預設跟隨系統；`ui/src/lib/appearance.ts` 在根元素標記 `data-appearance` 並記住設定，尺規與畫布底色（WASM `setWorkspaceColor`）一起切換。
- [x] **原生視窗外觀**：用 Tauri 的 app `setTheme` 設定整個 App 的 macOS 外觀，原生對話框與選單跟著切換。
- [x] **畫布不變色**：投影片內容維持簡報本身的配色，只有編輯器外框跟著切換（同 PowerPoint）。

### 3. 多語系（繁體中文、简体中文、English）

工作量最大（約 36 個檔案）；先建架構，再逐個元件抽字串。

- [x] **翻譯架構**：`ui/src/i18n/`，不依賴外部套件。啟動時依系統偏好語言（含 macOS 的個別 App 語言設定）選語系，找不到對應語系時退回 English；`zh-Hant.ts` 是鍵的來源，另兩種語言少翻任何一個鍵會編譯失敗；`t(key, params)` 支援 `{name}` 參數與英文單複數（`one|other`）。已套用：狀態列、存檔與復原對話框、檔案類型名稱、`APP_NAME`。
- [x] **抽出字串（前端）**：`ui/src` 所有介面文字改用 `t()`，約 600 個鍵，三種語言齊全；用語以 PowerPoint 各語言版為準。Ribbon 分頁內部仍以繁中名稱為識別值，只翻譯顯示文字。
- [x] **抽出字串（編輯器核心）**：`ppt-model::lang`（`Lang`、`tr()`、`plural()`）提供核心的語言，WASM 由前端 `setLanguage` 設定、原生端啟動時自行偵測。已翻譯：VoiceOver 播報與物件描述（`a11y.rs`）、協助工具檢查說明（`check.rs`）、選取窗格群組名稱、新物件預設名稱（TextBox 3／Picture 4／Table 5／Group 6）、`ppt-wasm` 錯誤訊息與縮圖標籤、未支援物件的提示、色彩配置名稱、PDF 預設標題、匯出視訊的錯誤訊息。
- [x] **英文版面檢查**：英文介面實際檢查 Ribbon、對話框、窗格，沒有截斷或換行問題。
- [x] **原生選單**：選單文字依語系切換；語言判斷改成和前端相同的規則（`Lang::detect`，依偏好語言順序找第一個支援的語系，否則英文）。
- [x] **日期與時間格式**：頁首頁尾日期、簡報者檢視時鐘、復原清單的時間依介面語系（`i18n` 的 `locale`）。
- [x] **新簡報預設文字**：佔位符提示（按一下以新增標題／Click to add title／单击此处添加标题）、範本的版面配置名稱、圖形名稱、母片文字、文字語言標記都依語系產生（前端啟動時呼叫 WASM `setLanguage`）。
- [x] **App 名稱**：Finder／Dock 用 `src-tauri/macos/*.lproj/InfoPlist.strings`，中文為「旺-Point」，英文為「Want-Point」。

### 4. 功能補強（視需要，不擋試用）

- [ ] **SmartArt**：其他版面的節點重排與插入（見「六、開發階段」M5.1；組織圖、金字塔與插入新 SmartArt 已完成，尚未用 PowerPoint 開啟驗證存檔結果）。
- [ ] **匯出影片**：投影片裡的音訊與影片、旁白錄製（見 M5.2）。

### 5. 公開發佈前（暫緩）

- [ ] **Developer ID 簽章與公證**：需要 Apple Developer 帳號（年費 99 美元）與 Developer ID Application 憑證；簽章後送 notarization 並 staple，別人下載後才不會被 macOS 擋下。帳號審核可能要幾天，決定公開發佈時先申請。
- [ ] **冷啟動時間重量**：加入深色主題與多語系後，用簽章版再量一次，確認仍 < 1 秒。
