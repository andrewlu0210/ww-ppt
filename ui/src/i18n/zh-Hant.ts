// Traditional Chinese: the source of truth for message keys. Every other
// language must translate each key (checked by the type of `Messages`).
// `{name}` is replaced by a parameter; "one|other" picks a plural form by `count`.

export const zhHant = {
  appName: "旺-Point",

  "status.slide": "投影片 {current} / {total}",
  "status.editingText": "編輯文字",
  "status.selected": "已選取：{name}",
  "status.loadingFonts": "正在載入字型…",
  "status.checkAccessibility": "檢查協助工具",
  "status.a11yGood": "協助工具：良好",
  "status.a11yIssues": "協助工具：{count} 個問題",
  "status.toggleNotes": "顯示或隱藏備忘稿",
  "status.notes": "備忘稿",
  "status.normal": "標準",
  "status.sorter": "投影片瀏覽",
  "status.zoomOut": "縮小",
  "status.zoomIn": "放大",
  "status.fit": "符合視窗",

  "file.pptxFilter": "PowerPoint 簡報",
  "file.imageFilter": "圖片",
  "file.pdfFilter": "PDF 文件",
  "file.mp4Filter": "MPEG-4 視訊",

  "save.ask": "要儲存「{name}」的變更嗎？",
  "save.save": "儲存",
  "save.dontSave": "不要儲存",
  "save.cancel": "取消",

  "recover.title": "復原文件",
  "recover.ask": "{app} 上次沒有正常結束。要復原「{name}」嗎？\n\n自動儲存的版本：{when}",
  "recover.recover": "復原",
  "recover.discard": "捨棄",
  "recover.later": "稍後再說",

  "title.unsaved": "有未儲存的變更",
};

export type Messages = Record<keyof typeof zhHant, string>;
