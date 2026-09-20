x = "global"


def reads_global():
    return x


def shadows():
    x = "local"
    return x


print(reads_global())
print(shadows())
print(x)


def counter_factory():
    total = 0

    def add(n):
        return total + n

    return add


add = counter_factory()
print(add(5))

print = print  # builtins are ordinary bindings
print("still works")
