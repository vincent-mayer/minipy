# Euclid's algorithm. minipy has no tuple assignment, so the swap is explicit.
def gcd(a, b):
    while b != 0:
        remainder = a % b
        a = b
        b = remainder
    return a


def lcm(a, b):
    return a * b // gcd(a, b)


print(gcd(1071, 462))
print(gcd(17, 5))
print(lcm(4, 6))
print(gcd(-12, 18))
