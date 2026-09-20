# Nested loops, break, and a flag standing in for for/else.
def is_prime(n):
    if n < 2:
        return False
    d = 2
    while d * d <= n:
        if n % d == 0:
            return False
        d += 1
    return True


count = 0
n = 2
while count < 15:
    if is_prime(n):
        print(n)
        count += 1
    n += 1

# The same search, stopping early with `break`.
first_above_100 = 0
for candidate in range(101, 200):
    if is_prime(candidate):
        first_above_100 = candidate
        break
print("first prime above 100:", first_above_100)
