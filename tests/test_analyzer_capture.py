"""Tests for raydriver.analyzer.capture."""

from raydriver.analyzer.capture import Capture, Frame, Recorder
from raydriver.core import SessionEvent


def test_frame_payload_roundtrip():
    text_frame = Frame("tx", 0.0, text="$I")
    binary_frame = Frame("rx", 0.1, data=b"\x88\xa8\x00")
    assert text_frame.payload() == b"$I"
    assert binary_frame.payload() == b"\x88\xa8\x00"


def test_capture_save_load_roundtrip(tmp_path):
    path = tmp_path / "cap.jsonl"
    capture = Capture(protocol="grbl", source="probe")
    capture.append("tx", text="$I")
    capture.append("rx", text="[VER:1.1h.20190825:]")
    capture.append("rx", data=b"\x88\xa8")
    capture.append_note("client connected from 127.0.0.1:5000")
    capture.save(path)

    loaded = Capture.load(path)
    assert loaded.protocol == "grbl"
    assert loaded.source == "probe"
    assert len(loaded.frames) == 4
    assert loaded.frames[0].text == "$I"
    assert loaded.frames[2].data == b"\x88\xa8"
    assert (loaded.frames[3].note or "").startswith("client connected")
    assert [f.offset for f in loaded.frames] == sorted(
        f.offset for f in loaded.frames
    )


def test_capture_load_rejects_garbage(tmp_path):
    path = tmp_path / "bad.jsonl"
    path.write_text('{"meta": {}}\n{"nonsense": true}\n')
    try:
        Capture.load(path)
    except ValueError as err:
        assert "bad frame record" in str(err)
    else:
        raise AssertionError("expected ValueError")


def test_recorder_records_raw_events_and_forwards():
    forwarded = []
    recorder = Recorder(forward=forwarded.append)
    recorder(SessionEvent.RawTx("$I"))
    recorder(SessionEvent.RawRx("ok"))
    directions = [f.direction for f in recorder.capture.frames]
    assert directions == ["tx", "rx"]
    assert recorder.capture.frames[0].text == "$I"
    assert len(forwarded) == 2
    assert forwarded[0].line == "$I"
    assert forwarded[1].line == "ok"
