# Generates a deck with a video and background music, to test video export.
# Usage: uv run --no-project --with python-pptx python scripts/make_media_pptx.py out.pptx clip.mp4 bgm.m4a
#   slide 1: the video plays by itself (volume 80%), trimmed 500 ms from its start
#   slide 2: background music that loops and carries over 2 slides, on a slide with a click-started video
#   slide 3: plain slide
import sys

from lxml import etree
from pptx import Presentation
from pptx.util import Inches

P = "http://schemas.openxmlformats.org/presentationml/2006/main"
R = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"


def timing(steps, media):
    """`steps`: list of (spid, click?) playFrom commands; `media`: <p:audio|video> settings."""
    pars = ""
    n = 10
    for spid, click, *rest in steps:
        delay = rest[0] if rest else 0
        cond = '<p:cond delay="indefinite"/>' if click else '<p:cond delay="indefinite"/><p:cond evt="onBegin" delay="0"><p:tn val="2"/></p:cond>'
        node = "clickEffect" if click else "afterEffect"
        pars += (
            f'<p:par><p:cTn id="{n}" fill="hold"><p:stCondLst>{cond}</p:stCondLst><p:childTnLst>'
            f'<p:par><p:cTn id="{n+1}" fill="hold"><p:stCondLst><p:cond delay="0"/></p:stCondLst><p:childTnLst>'
            f'<p:par><p:cTn id="{n+2}" presetID="1" presetClass="mediacall" presetSubtype="0" fill="hold" nodeType="{node}">'
            f'<p:stCondLst><p:cond delay="{delay}"/></p:stCondLst><p:childTnLst>'
            f'<p:cmd type="call" cmd="playFrom(0.0)"><p:cBhvr><p:cTn id="{n+3}" dur="1" fill="hold"/>'
            f'<p:tgtEl><p:spTgt spid="{spid}"/></p:tgtEl></p:cBhvr></p:cmd>'
            f"</p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn></p:par>"
        )
        n += 4
    return etree.fromstring(
        f'<p:timing xmlns:p="{P}"><p:tnLst><p:par><p:cTn id="1" dur="indefinite" restart="never" nodeType="tmRoot"><p:childTnLst>'
        f'<p:seq concurrent="1" nextAc="seek"><p:cTn id="2" dur="indefinite" nodeType="mainSeq"><p:childTnLst>{pars}</p:childTnLst></p:cTn>'
        f'<p:prevCondLst><p:cond evt="onPrev" delay="0"><p:tgtEl><p:sldTgt/></p:tgtEl></p:cond></p:prevCondLst>'
        f'<p:nextCondLst><p:cond evt="onNext" delay="0"><p:tgtEl><p:sldTgt/></p:tgtEl></p:cond></p:nextCondLst></p:seq>'
        f"{media}</p:childTnLst></p:cTn></p:par></p:tnLst></p:timing>"
    )


def media_node(kind, spid, vol, extra_node="", extra_ctn=""):
    return (
        f'<p:{kind}><p:cMediaNode vol="{vol}"{extra_node}><p:cTn id="90{spid}" fill="hold"{extra_ctn} display="0">'
        f'<p:stCondLst><p:cond delay="indefinite"/></p:stCondLst>'
        f'<p:endCondLst><p:cond evt="onStopAudio" delay="0"><p:tgtEl><p:sldTgt/></p:tgtEl></p:cond></p:endCondLst></p:cTn>'
        f'<p:tgtEl><p:spTgt spid="{spid}"/></p:tgtEl></p:cMediaNode></p:{kind}>'
    )


def set_timing(slide, tree):
    # python-pptx adds a bare timing of its own when a movie is added.
    for old in slide._element.findall(f"{{{P}}}timing"):
        slide._element.remove(old)
    slide._element.append(tree)


def add_trim(pic, st_ms):
    for el in pic._element.iter():
        if el.tag.endswith("}media"):
            trim = etree.SubElement(el, "{http://schemas.microsoft.com/office/powerpoint/2010/main}trim")
            trim.set("st", str(st_ms))


def main(out, video, audio):
    prs = Presentation()
    prs.slide_width, prs.slide_height = Inches(13.333), Inches(7.5)
    blank = prs.slide_layouts[6]

    s1 = prs.slides.add_slide(blank)
    v1 = s1.shapes.add_movie(video, Inches(2), Inches(1), Inches(6.4), Inches(3.6), mime_type="video/mp4")
    add_trim(v1, 500)
    set_timing(s1, timing([(v1.shape_id, False)], media_node("video", v1.shape_id, 80000)))

    s2 = prs.slides.add_slide(blank)
    a = s2.shapes.add_movie(audio, Inches(0.5), Inches(0.5), Inches(1), Inches(1), mime_type="audio/mp4")
    for el in a._element.iter():  # PowerPoint stores sound as a:audioFile
        if el.tag.endswith("}videoFile"):
            el.tag = el.tag.replace("videoFile", "audioFile")
    v2 = s2.shapes.add_movie(video, Inches(6), Inches(3), Inches(6.4), Inches(3.6), mime_type="video/mp4")
    media = media_node("audio", a.shape_id, 60000, ' numSld="2"', ' repeatCount="indefinite"') + media_node("video", v2.shape_id, 100000)
    set_timing(s2, timing([(a.shape_id, False), (v2.shape_id, True)], media))

    prs.slides.add_slide(blank)
    prs.save(out)


if __name__ == "__main__":
    main(*sys.argv[1:4])
