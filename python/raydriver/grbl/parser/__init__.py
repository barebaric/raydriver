import raydriver.raydriver as _raydriver  # type: ignore[import-untyped]


def __getattr__(name):
    return getattr(_raydriver.grbl.parser, name)
