s = "hello"
print(s, len(s), s * 2, s + "!")
print("a" + "b" + "c", "-" * 0, "x" * -3)
print("tab\there", "newline\nhere", "quote\"here", 'single\'quote')
print("backslash\\here")
for ch in "abc":
    print(ch, len(ch))
print(str(1), str(1.5), str(True), str(None))
print(int("42"), int(" 7 "), int(3.99), int(-3.99), int(True))
print(float("1.5"), float(3), float("-2e3"))
print(bool(""), bool("a"), bool(0), bool(0.0), bool(1))
