"""Tests for raydriver.analyzer.decode."""

from raydriver.analyzer.capture import Capture, Frame
from raydriver.analyzer.decode import (
    GrblDecoder,
    decoder_names,
    get_decoder,
    render_listing,
)


def grbl_frame(direction, payload, **kwargs):
    if isinstance(payload, str):
        return Frame(direction, 0.0, text=payload, **kwargs)
    return Frame(direction, 0.0, data=payload, **kwargs)


def decode(direction, payload):
    return GrblDecoder().decode_frame(grbl_frame(direction, payload))


def test_decode_grbl_tx():
    assert decode("tx", "?") == "status report request"
    assert decode("tx", "~") == "cycle start"
    assert decode("tx", "!") == "feed hold"
    assert decode("tx", "\x18") == "soft reset"
    assert decode("tx", "$I") == "build info request"
    assert decode("tx", "$$") == "settings request"
    assert decode("tx", "$J=G91 G21 F1000") == "jog"
    assert decode("tx", "$10=3") == "setting command"
    assert decode("tx", "G1 X10 F1000") == ""


def test_decode_grbl_rx():
    assert decode("rx", "ok") == "ack"
    assert decode("rx", "error:9").startswith("error: ")
    assert decode("rx", "ALARM:3").startswith("alarm: ")
    assert decode("rx", "[MSG:Caution:Unlocked]") == (
        "message Caution=Unlocked"
    )
    assert decode("rx", "[VER:1.1h.20190825:]").startswith("version 1.1h")
    assert "rx buffer 128" in decode("rx", "[OPT:V,15,128]")
    assert (
        decode("rx", "[PRB:10.000,20.000,0.000:1]")
        == "probe (found) at 10,20,0"
    )
    assert decode("rx", "[G54:0.000,0.000,0.000]") == "WCS G54 = 0,0,0"
    assert decode("rx", "$10 = 3") == "setting $10 = 3"


def test_decode_grbl_status_report():
    summary = decode("rx", "<Idle|MPos:12.500,0.000,3.000|Bf:15,128>")
    assert summary.startswith("IDLE MPos:12.500,0.000,3.000")
    summary = decode("rx", "<Run|MPos:1.0,2.0,0.0|Bf:15,128|F:1200>")
    assert summary.startswith("RUN MPos:1.000,2.000,0.000")


def test_decode_grbl_binary_and_empty():
    assert decode("rx", b"\x88\xa8") == ""
    assert decode("rx", "") == ""


def test_decoder_registry():
    assert "grbl" in decoder_names()
    assert "hex" in decoder_names()
    assert isinstance(get_decoder("grbl"), GrblDecoder)
    assert get_decoder("nonsense").name == "raw"


def test_hex_decoder():
    decoder = get_decoder("hex")
    frame = Frame("tx", 0.0, data=b"\x88\xa8\x01\x02\x03\x04\x05\x06\x07")
    summary = decoder.decode_frame(frame)
    assert summary == "88 a8 01 02 03 04 05 06 ..."


def test_render_listing_columns():
    capture = Capture(protocol="grbl")
    capture.append("tx", text="$I")
    capture.append("rx", text="ok")
    capture.append_note("client connected")
    listing = render_listing(capture, with_times=False)
    lines = listing.splitlines()
    assert lines[0].split() == ["#", "dir", "payload", "decode"]
    assert lines[1].endswith("build info request")
    assert lines[2].endswith("ack")
    assert lines[3].endswith("client connected")
    assert not any("\t" in line for line in lines)


def test_render_listing_with_times():
    capture = Capture(protocol="grbl")
    capture.append("tx", text="$I")
    listing = render_listing(capture)
    assert len(listing.splitlines()) == 2
    assert listing.splitlines()[0].split()[1] == "t"
