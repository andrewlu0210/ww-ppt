# Generates samples/m0-demo.pptx with python-pptx's default (Office) template.
# Usage: uv run --no-project --with python-pptx python scripts/make_sample_pptx.py samples/m0-demo.pptx
import io
import struct
import sys
import zlib

from pptx import Presentation
from pptx.dml.color import RGBColor
from pptx.enum.shapes import MSO_CONNECTOR, MSO_SHAPE
from pptx.enum.text import PP_ALIGN
from pptx.util import Inches, Pt


def gradient_png(w, h):
    rows = b"".join(
        b"\x00" + b"".join(bytes((int(255 * x / w), int(255 * y / h), 180)) for x in range(w)) for y in range(h)
    )

    def chunk(tag, data):
        return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)

    header = struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b"")


def main(out):
    prs = Presentation()
    prs.slide_width, prs.slide_height = Inches(13.333), Inches(7.5)

    # 1. Title slide
    s = prs.slides.add_slide(prs.slide_layouts[0])
    s.shapes.title.text = "ww-ppt M0 技術驗證"
    s.placeholders[1].text = "Rust + Tauri v2 + WebGPU · 中英混排 Mixed Text"

    # 2. Title and content: bullet levels and mixed runs
    s = prs.slides.add_slide(prs.slide_layouts[1])
    s.shapes.title.text = "核心目標"
    tf = s.placeholders[1].text_frame
    tf.text = "排版精準度：與 PowerPoint 一致"
    for text, lvl in [("文字換行、字距、行距", 1), ("操作延遲：拖曳 120fps", 0), ("PPTX 相容性", 0), ("讀不懂的保留、不要破壞", 1)]:
        p = tf.add_paragraph()
        p.text = text
        p.level = lvl
    p = tf.add_paragraph()
    r = p.add_run()
    r.text = "粗體 "
    r.font.bold = True
    r = p.add_run()
    r.text = "紅色 "
    r.font.color.rgb = RGBColor(0xC0, 0x00, 0x00)
    r = p.add_run()
    r.text = "italic underline"
    r.font.italic = True
    r.font.underline = True

    # 3. Shapes, rotation, connector, text box, group
    s = prs.slides.add_slide(prs.slide_layouts[5])
    s.shapes.title.text = "圖形與變形"
    r1 = s.shapes.add_shape(MSO_SHAPE.ROUNDED_RECTANGLE, Inches(0.8), Inches(2), Inches(3), Inches(1.6))
    r1.text_frame.text = "圓角矩形"
    e = s.shapes.add_shape(MSO_SHAPE.OVAL, Inches(4.5), Inches(2), Inches(2.4), Inches(1.6))
    e.fill.solid()
    e.fill.fore_color.rgb = RGBColor(0x70, 0xAD, 0x47)
    e.text_frame.text = "Ellipse"
    a = s.shapes.add_shape(MSO_SHAPE.RIGHT_ARROW, Inches(7.5), Inches(2.2), Inches(2.5), Inches(1.2))
    a.rotation = 20
    st = s.shapes.add_shape(MSO_SHAPE.STAR_5_POINT, Inches(10.6), Inches(1.9), Inches(1.8), Inches(1.8))
    st.fill.solid()
    st.fill.fore_color.rgb = RGBColor(0xFF, 0xC0, 0x00)
    c = s.shapes.add_connector(MSO_CONNECTOR.STRAIGHT, Inches(1), Inches(5.6), Inches(5), Inches(4.4))
    c.line.width = Pt(3)
    c.line.color.rgb = RGBColor(0x44, 0x72, 0xC4)
    tb = s.shapes.add_textbox(Inches(6), Inches(4.4), Inches(6), Inches(1.5))
    tb.text_frame.word_wrap = True
    p = tb.text_frame.paragraphs[0]
    p.alignment = PP_ALIGN.CENTER
    r = p.add_run()
    r.text = "文字方塊：這段文字應該在方塊內自動換行，並且置中對齊。"
    r.font.size = Pt(24)
    g = s.shapes.add_group_shape()
    g.shapes.add_shape(MSO_SHAPE.RECTANGLE, Inches(0.8), Inches(6.2), Inches(1), Inches(0.8))
    g.shapes.add_shape(MSO_SHAPE.ISOSCELES_TRIANGLE, Inches(2), Inches(6.2), Inches(1), Inches(0.8))

    # 4. Picture
    s = prs.slides.add_slide(prs.slide_layouts[5])
    s.shapes.title.text = "圖片"
    s.shapes.add_picture(io.BytesIO(gradient_png(320, 200)), Inches(3.5), Inches(2), Inches(6.4), Inches(4))

    prs.save(out)
    print("wrote", out)


if __name__ == "__main__":
    main(sys.argv[1])
