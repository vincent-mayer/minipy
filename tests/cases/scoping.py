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


# A name a function binds is local for the whole function, so the outer one
# is untouched.
def rebinds():
    x = "inner"
    return x


print(rebinds(), x)


# Reads still walk outward when the function does not bind the name.
depth = "module"


def reads():
    return depth


def nests():
    depth = "function"

    def inner():
        return depth

    return inner()


print(reads(), nests())


# Each call gets its own frame.
def accumulate(n):
    total = 0
    for i in range(n):
        total += i
    return total


print(accumulate(4), accumulate(5))
