import raydriver.raydriver as _raydriver


def __getattr__(name):
    return getattr(_raydriver.driver, name)
