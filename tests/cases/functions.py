def no_return():
    pass


def early(n):
    if n > 0:
        return "positive"
    return "non-positive"


def recurse(n):
    if n == 0:
        return 0
    return n + recurse(n - 1)


print(no_return())
print(early(1), early(-1))
print(recurse(100))


def outer():
    captured = "outer value"

    def inner():
        return captured

    return inner


print(outer()())


def apply_twice(f, x):
    return f(f(x))


def square(x):
    return x * x


print(apply_twice(square, 3))


# A function defined inside a loop captures the scope it was defined in.
def make():
    n = 0

    def get():
        return n

    n = 99
    return get()


print(make())
