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
     - 插入新的 SmartArt：「插入 > SmartArt」可選二十種版面：垂直項目符號清單（`vList2`）、水平項目符號清單（`hList1`）、群組清單（`lProcess2`）、基本流程（`process1`）、垂直流程（`process2`）、流程箭號（`process3`）、連續區塊流程（`hProcess9`）、逐步向下流程（`StepDownProcess`）、放射狀清單（`radial2`）、基本人字形流程（`chevron1`）、封閉式人字形流程（`hChevron3`）、垂直人字形清單（`chevron2`）、基本循環（`cycle2`）、連續循環（`cycle3`）、區塊循環（`cycle5`）、分段循環（`cycle8`）、基本金字塔圖（`pyramid1`）、組織圖（`orgChart1`）、層次結構（`hierarchy1`）、水平層次結構（`hierarchy2`）。每種版面內嵌一組起手式（資料、版面定義、色彩、樣式、繪圖五個部件，取自既有簡報，文字清掉，見 `scripts/extract_smartart_starters.py` 與 `crates/ppt-ooxml/assets/smartart/`）；插入時把部件放進簡報套件、縮放到合適大小，再走一般的載入流程讀成 SmartArt，所以之後的編輯與存檔和開檔載入的完全相同，插入是一步復原。起手式是 3–4 個節點（組織圖是根加兩個部門各兩個小組），文字為「項目 1」等。
     - 拼字檢查：節點文字畫在自己的圖形裡時（例如清單、流程、組織圖、金字塔、循環的方框），拼錯的字會畫紅色波浪線，右鍵可更正，更正是一步復原；在畫布上編輯該節點時檢查的是即時文字。顯示在父節點圖形裡的子項目文字（如 `hList1` 的項目符號）、以及被旋轉或翻轉的 SmartArt 不檢查。
     - 層次結構（`hierarchy1`）：和組織圖同一套樹狀重排，節點可由多個圖形組成（方框加陰影，相對位置不變），連接線重畫；已可插入。
     - 水平層次結構（`hierarchy2`）：樹向右長，各層的欄位位置與寬度、子樹之間的間距從載入的圖形量出；連接線是從父節點右緣到子節點左緣的直線（旋轉過的線段，依兩端算出長度與角度）；已可插入。
     - 連續循環（`cycle3`）：節點沿圓等距重排，環繞整圈的圓弧箭頭只有一個、跟著第一個節點，首尾相對第一個節點的距離按節點間距的比例移動（比例從載入的圖形量出）；節點數改變使環超出框時整環（節點一起）向圓心收攏。已可插入。
     - 區塊循環（`cycle5`）：節點沿圓等距重排，節點之間各有一段帶箭頭的圓弧，依新位置重算：弧從上一個節點的方框離開圓處起、到下一個節點的方框進入圓處止，兩端各留載入時量出的距離；弧以貝茲曲線寫出。已可插入。
     - 分段循環（`cycle8`）：每個節點一片扇形加一條外緣的弧形箭頭。扇形等分整圈（起點、由中心外推的距離從載入的圖形量出）、箭頭首尾跟著扇形，文字放在扇形內可容納的最大直立矩形裡（以扇形角度搜尋，與載入圖形的差距量一次再帶過去）；存檔時重建整組 pres 點（扇形的點掛在文件上、靠 `presOf` 對到節點），文字框（`dsp:txXfrm`）依新的內縮寫回。已可插入。
     - 另外修了 SmartArt 文字框的一個舊問題：`dsp:txXfrm` 折進內縮後，又被預設幾何的文字矩形再縮一次（扇形最明顯，文字被擠成直排）；現在 `TextBody::exact_frame` 標示這種文字，改從圖形邊界量起。
     - 放射狀清單（`radial2`）：中間一個 hub，每個頂層節點（圓形標題加放子項目文字的方框）依個數落在 hub 右側的圓弧上（1 個在正右、2 個 70°–110°、3 個 60°–120°、更多 45°–135°，角度由版面定義的 cycle 規則得來，從正上方順時針量）；半徑、圖形大小取載入時的值，弧放不下時整組（含字型）繞 hub 縮小；hub 到標題的細線依新位置重算（從 hub 連接圓的邊到標題的邊）。子項目可新增刪除（顯示在方框裡）。存檔時重建每個節點的線、圖組與 `presOf`。已可插入。
     - 逐步向下流程（`StepDownProcess`）：每個節點是一組圖形（彎箭頭、標題、放子項目的方框），一步一步往右下排，最後一個只有標題；每步的位移、組內各圖形相對標題的位置取載入時的值。整座階梯放大或縮小到填滿框的寬或高（放大最多 2 倍，字型一起），並置中。存檔走一般的流程版面的 pres 點規劃。已可插入。
     - 群組清單（`lProcess2`）：節點是並排的高欄，每欄上方是標題、下面是每個子項目一個方框。欄依個數平分框的寬度（欄距比例從載入的圖形量出，即版面定義的 0.075），方框平分標題下方的空間，個數多時一起縮小（間距為版面定義的 0.1 倍高，等比縮）；孫項目顯示在子項目的方框裡。存檔時重建每欄的整組 pres 點，方框的樣式序號按全部子項目的順序重排。已可插入。樣本：`samples/vr/test17.pptx`（與 PowerPoint 匯出的 PDF）。
     - PowerPoint 驗證（2026-11-01）：`powerpoint-check.md` 是驗證清單，`smartart_check` 範例產生要開的檔案（20 種版面各一張投影片，第 N 張就是清單第 N 項）。使用者回報基本都通過：開啟沒有修復提示、PowerPoint 認得是 SmartArt、結構與層級正確；逐張截圖確認過的有 `hierarchy2`、`cycle3`、`cycle5`。從這些截圖看，PowerPoint 開啟時是依資料模型與版面定義自己重排，不直接用我們存的繪圖，所以資料模型（節點、`parOf`、pres 點）正確最重要，繪圖位置只影響本 App 與其他看圖軟體。
     - 新增的節點依位置循環取色（PowerPoint 的做法）：節點 j 取載入的第 j 個（超過個數就循環）節點的填色，存檔時也把填色寫回繪圖。
     - SmartArt 版面的節點重排已全部完成（之前列為待補的版面都做完了）；其他沒有遇到的版面仍只能改文字。
   - **M5.2 匯出影片**：可選解析度、每頁停留秒數或照排練時間；逐格算出轉場與動畫畫面，由 Tauri 端用 AVFoundation 編成 H.264 `.mp4`，顯示進度、可取消。
     - 已完成：檔案選單與「投影片放映」分頁的「匯出為視訊」、4K／1080p／720p／480p、每張秒數與錄製時間、轉場與動畫依序播放、跳過隱藏投影片。
     - 已完成（2026-11-03）：投影片裡的影片與音訊（含背景音樂）會進到匯出的視訊。從原始 XML 讀播放設定（`ppt-ooxml::media_clips`：音量、循環、跨投影片、自動或點擊開始、`p14:trim` 裁剪）；聲音在 Rust 混成一軌（`ppt-video::mix`），經 WAV 交給 AVAssetWriter 編成 AAC；影片用 AVAssetImageGenerator 逐格取畫面貼到物件位置。不循環的媒體會把所在投影片的停留時間撐到播完。限制：影片畫面不處理旋轉與裁切，且蓋在其他物件上方；`p14:trim` 的 `st`、`end` 依 MS-PPTX 當成「從頭、從尾裁掉的毫秒」，尚未用真正的 PowerPoint 檔驗證；點擊開始的媒體在匯出時從該次點擊的動畫開始處播放；播放設定以載入時的 XML 為準（編輯器不能編輯媒體）。
     - 待補：旁白錄製。
   - ~~M5.3 Operation 層~~、~~M5.4 協作（CRDT）~~、~~M5.5 外掛~~：**決定不做**（2026-10-30）。Operation 層原本是協作與外掛的前置，兩者不做後一併取消；undo 維持快照式。

## 七、主要風險

1. **文字排版跟 PowerPoint 對不齊**：Office 有自己的換行和字距演算法，不可能 100% 一樣。需要用大量樣本反覆比對調整。
2. **無障礙**：畫布對 VoiceOver 不可見，要另外維護一棵隱藏的 ARIA DOM 樹。
3. **WebGPU 相容性**：舊版 macOS 要有 tiny-skia → Canvas2D 的 CPU 退回方案。
4. **工程量**：完整的 PowerPoint 是好幾年、多人的規模。建議 M0、M1 先把範圍收窄到「能好好編輯和放映常見簡報」。
