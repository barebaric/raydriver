"""Remote sessions: a device-host daemon and a client proxy.

Submodules
----------
- raydriver.remote.protocol — wire framing, envelope, catalog and
  codecs (v1)
- raydriver.remote.daemon — session host: listener, registry,
  lease, event ring and fan-out
- raydriver.remote.client — RemoteSession proxy, the protocol-agnostic
  open_session / list_devices / list_protocols API, and the
  create_session factory
- raydriver.remote.mdns — optional _raydriver._tcp announce and
  browse (extra: daemon)
- raydriver.remote.inspect — read-only daemon inspection for the
  debug CLI

The protocol is the daemon's knowledge: a client opens a session by
device reference (open_session) and learns the protocol and its
capabilities from the daemon as info (raydriver.protocols), rather
than naming a driver itself.
"""
