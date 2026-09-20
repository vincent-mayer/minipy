# Closures: each `make_adder` call captures its own `start`.
def make_adder(start):
    def plus(n):
        return start + n

    return plus


add_10 = make_adder(10)
add_100 = make_adder(100)

print(add_10(1), add_10(2))
print(add_100(1), add_100(2))


def compose(f, g):
    def composed(x):
        return f(g(x))

    return composed


def double(x):
    return x * 2


def increment(x):
    return x + 1


print(compose(double, increment)(5))
print(compose(increment, double)(5))
