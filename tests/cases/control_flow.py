total = 0
for i in range(10):
    if i % 2 == 0:
        continue
    if i > 7:
        break
    total += i
print("total", total)

n = 0
while True:
    n += 1
    if n >= 4:
        break
print("n", n)

for i in range(3):
    for j in range(3):
        if j == 2:
            break
        print(i, j)

if 0:
    print("no")
elif "":
    print("no")
elif 1:
    print("yes")
else:
    print("no")

x = 5
if x > 3: print("one-liner"); print("same line")
