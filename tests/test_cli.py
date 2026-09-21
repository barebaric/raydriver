"""Tests for the raydriver command line interface."""

import pytest

from raydriver.cli.main import amain


@pytest.fixture
def gcode_file(tmp_path):
    def write(text):
        path = tmp_path / "job.gcode"
        path.write_text(text)
        return str(path)

    return write


class TestRunCommand:
    async def test_successful_job_shows_progress(self, gcode_file, capsys):
        path = gcode_file("G1 X10 F1000\nG1 Y5 F1000\nG1 X0 F1000\n")
        rc = await amain(
            ["run", path, "--emulator", "--speed-factor", "100000"]
        )
        assert rc == 0
        out = capsys.readouterr().out
        assert "3/3" in out
        assert "done" in out
        assert "Connecting to emulated device" in out
        assert "[VER:" in out

    async def test_progress_percentages_render(self, gcode_file, capsys):
        lines = "\n".join(f"G1 X{i % 30} F1000" for i in range(50))
        path = gcode_file(lines)
        rc = await amain(
            ["run", path, "--emulator", "--speed-factor", "20000"]
        )
        assert rc == 0
        out = capsys.readouterr().out
        assert "100.0%" in out
        assert "50/50" in out

    async def test_device_error_aborts_with_exit_code(
        self, gcode_file, capsys
    ):
        path = gcode_file("G1 X10 F1000\nG999\nG1 X0 F1000\n")
        rc = await amain(["run", path, "--emulator", "--speed-factor", "100"])
        assert rc == 1
        out = capsys.readouterr().out
        assert "ABORTED" in out

    async def test_empty_file_is_rejected(self, gcode_file, capsys):
        path = gcode_file("; only comments\n\n")
        rc = await amain(["run", path, "--emulator"])
        assert rc == 2
        assert "no G-code lines" in capsys.readouterr().err

    async def test_missing_device_fails_cleanly(self, gcode_file):
        path = gcode_file("G1 X10 F1000\n")
        with pytest.raises(ConnectionError):
            # No emulator and a serial port that cannot answer.
            await amain(
                [
                    "run",
                    path,
                    "--port",
                    "/dev/null-does-not-exist",
                    "--timeout",
                    "0.3",
                ]
            )


class TestStatusCommand:
    async def test_status_reports_printed(self, capsys):
        rc = await amain(["status", "--emulator", "--seconds", "0.5"])
        assert rc == 0
        out = capsys.readouterr().out
        assert "IDLE" in out
        assert "MPos:" in out
