import random

f = open("largedisputes.csv", "a")
f.write("type, client, tx, amount\n")

# u16 max
client_id = 65535
# u32 max, count down
curr_tx = 4294967295

# count what the value should be at the end
available = 0
disputed = 0
total = 0

deposited = {}
withdrew = {}
disputes = {}
resolved = {}
chargeback = {}


for i in range(100000):
    amount = round(random.uniform(0.0001, 1), 4)
    deposited[curr_tx] = amount
    available += amount
    total += amount
    f.write(f'deposit,{client_id},{curr_tx},{amount}\n')
    curr_tx -= 1

for i in range(5000):
    amount = round(random.uniform(0.0001, 0.5), 4)
    withdrew[curr_tx] = amount
    available -= amount
    total -= amount
    f.write(f'withdrawal,{client_id},{curr_tx},{amount}\n')
    curr_tx -= 1

for i in range(5000):
    tx_id, amount = deposited.popitem()
    disputes[tx_id] = amount
    available -= amount
    disputed += amount
    f.write(f'dispute,{client_id},{tx_id},\n')

for i in range(2000):
    tx_id, amount = disputes.popitem()
    resolved[tx_id] = amount
    available += amount
    disputed -= amount
    f.write(f'resolve,{client_id},{tx_id},\n')

for i in range(2000):
    tx_id, amount = disputes.popitem()
    chargeback[tx_id] = amount
    total -= amount
    disputed -= amount
    f.write(f'chargeback,{client_id},{tx_id},\n')

print(f'available: {available}, disputed: {disputed}, total: {total}')



# sanity check, also tally the dicts
# available = 0
# disputed = 0
# total = 0
# 
# for _, v in deposited.items():
#     available += v
#     total += v
# 
# for _, v in withdrew.items():
#     available -= v
#     total -= v
# 
# for _, v in disputes.items():
#     disputed += v
#     total += v
# 
# for _, v in resolved.items():
#     available += v
#     total += v
# 
# print(f'sanity check: {available}, disputed: {disputed}, total: {total}')

f.close()


f = open("largedisputes.csv.expected", "a")
available = round(available, 4)
disputed = round(disputed, 4)
total = round(total, 4)

f.write("client,available,held,total,locked\n")
f.write(f"{client_id},{available},{disputed},{total},true\n")
f.close()
