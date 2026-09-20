a = 1


def outer():
    def inner():
        return a

    print(inner())
    a = 2


outer()
