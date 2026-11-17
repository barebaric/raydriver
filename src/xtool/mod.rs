//! The xTool protocol stack for xTool's proprietary WiFi module
//! (D1, D1 Pro, F1, S1, …).
//!
//! xTool machines are GRBL-derived but their network API is *not*
//! ESP3D or GRBL-over-TCP: single commands are triggered with
//! `GET /cmd?cmd=…`, multi-line blocks with `POST /cmd`, jobs are
//! uploaded to `/tmp.gcode` via multipart `/upload` and started with
//! `/read`, and pause/resume/stop go through `/cnc/data`.  The HTTP
//! API acknowledges every request with a JSON body whose `result`
//! member is `"ok"` or `"fail"` — it never returns the GRBL response
//! text of the executed command.
//!
//! The WebSocket (default port 8081) is a one-way notification
//! channel: it carries job-state events (`ok:IDLE`, `ok:WORKING_*`,
//! `ok:PAUSING`, `WORK_STOPPED`) and safety alarms (`err:*`), but
//! never per-line `ok` acknowledgements and never `<Idle|…>` status
//! reports.  The machine therefore reports no position at all; the
//! session state is derived entirely from the job-state events.

pub mod http;
pub mod session;
pub mod ws;
