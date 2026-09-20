# Strings: concatenation, repetition, iteration, comparison.
banner = "-" * 24
print(banner)

word = "interpreter"
print(word, len(word))

vowels = 0
for ch in word:
    if ch == "a" or ch == "e" or ch == "i" or ch == "o" or ch == "u":
        vowels += 1
print("vowels:", vowels)

reversed_word = ""
for ch in word:
    reversed_word = ch + reversed_word
print(reversed_word)

print("apple" < "banana", "a" * 3 == "aaa")
print(str(42) + "!", int("17") + 1, float("2.5") * 2)
print(banner)
