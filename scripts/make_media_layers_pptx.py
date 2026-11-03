# Generates a deck for video export's picture handling: one slide with a rotated, cropped video
# whose click-less start is delayed 1 s, a shape under it and a shape over it.
# Usage: uv run --no-project --with python-pptx python scripts/make_media_layers_pptx.py out.pptx clip.mp4
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from make_media_pptx import media_node, set_timing, timing  # noqa: E402
from pptx import Presentation  # noqa: E402
from pptx.dml.color import RGBColor  # noqa: E402
from pptx.enum.shapes import MSO_SHAPE  # noqa: E402
from pptx.util import Inches  # noqa: E402


def main(out, video):
    prs = Presentation()
    prs.slide_width, prs.slide_height = Inches(13.333), Inches(7.5)
    slide = prs.slides.add_slide(prs.slide_layouts[6])

    def box(shape_type, x, y, w, h, color):
        s = slide.shapes.add_shape(shape_type, Inches(x), Inches(y), Inches(w), Inches(h))
        s.fill.solid()
        s.fill.fore_color.rgb = RGBColor(*color)
        s.line.fill.background()
        return s

    box(MSO_SHAPE.RECTANGLE, 2.5, 1.2, 4.5, 3.4, (0x2E, 0xA0, 0x4F))  # under the video
    pic = slide.shapes.add_movie(video, Inches(3.5), Inches(2), Inches(4.5), Inches(2.5), mime_type="video/mp4")
    pic.rotation = 20
    pic.crop_left = 0.15
    pic.crop_right = 0.15
    box(MSO_SHAPE.RECTANGLE, 2.2, 3.4, 3.2, 1.6, (0xD0, 0x30, 0x30))  # over the video
    set_timing(slide, timing([(pic.shape_id, False, 1000)], media_node("video", pic.shape_id, 100000)))
    prs.save(out)


if __name__ == "__main__":
    main(*sys.argv[1:3])
