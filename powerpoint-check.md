# SmartArt 的 PowerPoint 驗證清單

我們在這裡驗證不了 PowerPoint：測試只證明「存檔後本程式能重新讀回」，不證明 PowerPoint 會接受。
這份清單用來在有 PowerPoint 的機器上，一次檢查所有 SmartArt 的新增、刪除與插入。

## 一、產生要開的檔案

```
cargo run -p ppt-vr --release --example smartart_check -- target/pp-check
```

會得到：

- `target/pp-check/smartart-all.pptx`：20 張投影片，**每種可插入的版面一張**。每張都是「插入新的 SmartArt」之後，再依固定步驟編輯（見第四節的表）：加一個頂層節點、有層級的版面加一個子項目、頂層夠多時刪掉第二個。
- `target/pp-check/smartart-all/NN-版面.png`：本程式畫出來的樣子，用來和 PowerPoint 的畫面對照（只看大致排法，不要求像素一致）。
- 終端機印出的表就是第四節的表（節點數可能因版面起手式不同而變）。

另外，用私人樣本跑 `diagram` 範例會得到「把每個 SmartArt 多加一個節點」的檔案（不會進 repo，只在你的機器上）：

```
cargo run -p ppt-vr --release --example diagram -- samples/vr/test8.pptx  target/diag8     # 組織圖
cargo run -p ppt-vr --release --example diagram -- samples/vr/test9.pptx  target/diagpyr   # 金字塔
cargo run -p ppt-vr --release --example diagram -- samples/vr/test10.pptx target/diagh     # 層次結構
cargo run -p ppt-vr --release --example diagram -- samples/vr/test11.pptx target/diagh2    # 水平層次結構
cargo run -p ppt-vr --release --example diagram -- samples/vr/test12.pptx target/diagc3    # 連續循環
cargo run -p ppt-vr --release --example diagram -- samples/vr/test13.pptx target/diagc5    # 區塊循環
cargo run -p ppt-vr --release --example diagram -- samples/vr/test14.pptx target/diagc8    # 分段循環
cargo run -p ppt-vr --release --example diagram -- samples/vr/test15.pptx target/diagr2    # 放射狀清單
cargo run -p ppt-vr --release --example diagram -- samples/vr/test16.pptx target/diagsd    # 逐步向下流程
cargo run -p ppt-vr --release --example diagram -- samples/vr/test17.pptx target/diagl2    # 群組清單
```

每個資料夾裡的 `edited.pptx` 是結果。這些檔案的原圖來自真實簡報，**最能測出 PowerPoint 對真實資料的反應**，建議也開。

## 二、先看最重要的一件事：有沒有「修復」

對每個檔案：

1. 用 PowerPoint 開啟。
2. **是否跳出「PowerPoint 發現內容有問題，要修復嗎」或類似訊息。** 有，就是失敗，記下是哪個檔案、哪張投影片（可以逐張複製到新檔來找）。
3. 修復後 SmartArt 是否消失、變成圖片、或變空白。

這一關沒過，後面都不用看；先回報。

## 三、每一張投影片都要看的項目

| # | 檢查 | 預期 |
|---|---|---|
| 1 | 畫面 | 圖形都在，沒有消失、重疊成一團，也沒有跑到框外；文字沒有被切掉到看不出是什麼字 |
| 2 | 點選圖形 | 出現 SmartArt 的設計／格式分頁（表示 PowerPoint 認得它是 SmartArt，不是一堆圖形） |
| 3 | 開啟文字窗格 | 節點的層級和數量與第四節表一致，文字是我們寫的（「項目 1」、「新增 A」…） |
| 4 | 在 PowerPoint 裡改一個字 | 文字正常更新，版面沒有整個亂掉 |
| 5 | 在 PowerPoint 裡新增一個節點 | 版面重排，新節點出現；這代表資料模型是 PowerPoint 認得的 |
| 6 | 在 PowerPoint 裡刪一個節點 | 同上 |
| 7 | 另存新檔再開 | 內容不變，沒有修復提示 |
| 8 | 與 `smartart-all/NN-*.png` 比較 | 排法大致相同（見第四節各版面的重點）。**PowerPoint 在你改動後會依自己的規則重排，位置和我們不同是正常的**；要看的是第 4、5 項之前，剛開啟時的畫面 |

第 2、3 項若失敗，通常表示 `dgm` 資料或關聯有誤；第 1、8 項若失敗，通常是繪圖（`dsp`）部分的幾何有誤。回報時請說明是哪一項。

## 四、各版面的重點

節點數是「插入時 → 編輯後」，來自 `smartart_check` 的輸出。重排規則的細節在 `ppt-plan.md`。

| 張 | 版面 | 節點數 | 這張特別要看 |
|---|---|---|---|
| 1 | `vList2` 垂直項目符號清單 | 3 → 3 | 方框垂直等距排列；刪除後間距重新均分 |
| 2 | `chevron1` 基本人字形流程 | 3 → 3 | 人字形接得起來，沒有縫隙或重疊 |
| 3 | `hList1` 水平項目符號清單 | 9 → 8 | 子項目是上方標題下面的項目符號，不是獨立圖形 |
| 4 | `cycle2` 基本循環 | 4 → 4 | 節點在圓上等距，箭頭在兩節點之間 |
| 5 | `cycle3` 連續循環 | 4 → 4 | **只有一個**環繞的大圓弧箭頭；它的首尾貼著第一個節點的兩側 |
| 6 | `cycle5` 區塊循環 | 4 → 4 | 每兩個節點之間各有一段帶箭頭的弧；弧不穿過節點方框 |
| 7 | `cycle8` 分段循環 | 4 → 4 | 扇形等分整圈，每片外緣一個弧形箭頭；文字在扇形裡、讀得出來 |
| 8 | `pyramid1` 基本金字塔圖 | 3 → 3 | 各層平分高度，斜邊接成連續的金字塔，最上面是三角形 |
| 9 | `orgChart1` 組織圖 | 7 → 9 | 根在上、部門一排、小組吊在部門下面；連接線是彎折線，接在方框上 |
| 10 | `hierarchy1` 層次結構 | 7 → 9 | 每個方框都有陰影（兩層圖形一起移動）；連接線接得上 |
| 11 | `hierarchy2` 水平層次結構 | 3 → 5 | 樹向右長；連接線是從父節點右緣到子節點左緣的斜線，端點貼著方框 |
| 12 | `lProcess2` 群組清單 | 9 → 8 | 高欄並排，標題在上，子項目方框在標題下；子項目變多時方框一起變矮 |
| 13 | `process1` 基本流程 | 3 → 3 | 方框之間有箭頭 |
| 14 | `process2` 垂直流程 | 3 → 3 | 垂直排列，箭頭向下 |
| 15 | `process3` 流程箭號 | 7 → 6 | 標題加子項目方框，箭號連接 |
| 16 | `hChevron3` 封閉式人字形流程 | 3 → 3 | 人字形連成一條 |
| 17 | `chevron2` 垂直人字形清單 | 9 → 8 | 人字形（旋轉 90 度）在左，子項目方框在右 |
| 18 | `hProcess9` 連續區塊流程 | 3 → 3 | 大箭頭底、方框在上 |
| 19 | `radial2` 放射狀清單 | 6 → 6 | 中間 hub，節點在右側圓弧上；hub 到標題的細線 |
| 20 | `StepDownProcess` 逐步向下流程 | 4 → 4 | 階梯往右下，每階有彎箭頭；最後一階只有標題 |

## 五、判斷與回報

- **通過**：第二節沒有修復提示，第三節 1–7 都通過。
- **部分通過**：沒有修復提示，但第三節 4–6（PowerPoint 裡再編輯）有問題 → 資料模型有缺。
- **失敗**：有修復提示或 SmartArt 消失。

回報請附：

1. 檔案與投影片編號（或版面名稱）。
2. 失敗的項目編號（第二節或第三節）與看到的畫面（截圖最好）。
3. PowerPoint 的版本（Mac 或 Windows，哪一版）。

有了這些，我可以回頭對照該版面存出的 XML，找出是哪一段寫錯了。

## 六、已知的、不算失敗的差異

- 我們的重排是**近似** PowerPoint 的規則：位置、間距、字型大小可能和 PowerPoint 自己排的不同；PowerPoint 在你編輯之後就會用它自己的規則重排。
- 字型大小是我們依方框大小估計的（插入後會依文字縮小到放得下），PowerPoint 自己排會用它的演算法，字大小不同是正常的。
- 刪除／新增節點後，節點的 ID 會換成我們產生的 ID（看起來像 GUID），這是正常的。
