import random

f = open("largetest.csv", "a")
f.write("type, client, tx, amount\n")

for i in range(10000000):
    client = random.randint(0, 65535)
    amount = round(random.uniform(0.0001, 100000.0), 4)
    f.write(f'deposit,{client},{i},{amount}\n')

for i in range(10000000, 20000000):
    client = random.randint(0, 65535)
    amount = round(random.uniform(0.0001, 100000.0), 4)
    f.write(f'withdrawal,{client},{i},{amount}\n')

for i in range(20000000, 30000000):
    client = random.randint(0, 65535)
    tx = random.randint(0, 10000000)
    f.write(f'dispute,{client},{tx},0\n')
    
for i in range(30000000, 40000000):
    client = random.randint(0, 65535)
    tx = random.randint(0, 10000000)
    f.write(f'chargeback,{client},{tx},\n')

for i in range(40000000, 50000000):
    client = random.randint(0, 65535)
    tx = random.randint(0, 10000000)
    f.write(f'resolve,{client},{tx},0.000\n')

f.close()
