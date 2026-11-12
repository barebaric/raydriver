"""Remote sessions: a device-host daemon and a client proxy.

Submodules
----------
- raydriver.remote.protocol — wire framing, envelope, catalog and
  codecs (v1)
- raydriver.remote.daemon — session host: listener, registry,
  lease, event ring and fan-out
- raydriver.remote.client — RemoteSession proxy and the
  create_session factory
"""
