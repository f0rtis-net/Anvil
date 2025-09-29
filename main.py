import random

def F(x2: int) -> int:
    low = x2 & 0xFF          
    high = (x2 >> 8) & 0xFF  

    part1 = (low >> 3) & 0xFF
    part2 = (~high) & 0xFF

    return (part2 << 8) | part1  


def round_key(K: int, i: int) -> int:
    tmp = (~K & ((1 << 64) - 1)) << (i * 2) 
    return (tmp >> 16) & 0xFFFF             


def feistel_round(X: list[int], k: int) -> list[int]:
    x0, x1, x2, x3 = X

    y0 = (x0 ^ x1) ^ (x3 ^ k)
    y1 = F(x2)
    y2 = x0
    y3 = x2

    return [y0 & 0xFFFF, y1 & 0xFFFF, y2 & 0xFFFF, y3 & 0xFFFF]


def encrypt(block: int, K: int, rounds: int) -> int:
    X = [
        (block >> 48) & 0xFFFF,
        (block >> 32) & 0xFFFF,
        (block >> 16) & 0xFFFF,
        block & 0xFFFF,
    ]

    for i in range(rounds):
        k = round_key(K, i)
        X = feistel_round(X, k)

    return (X[0] << 48) | (X[1] << 32) | (X[2] << 16) | X[3]


def decrypt(block: int, K: int, rounds: int) -> int:
    X = [
        (block >> 48) & 0xFFFF,
        (block >> 32) & 0xFFFF,
        (block >> 16) & 0xFFFF,
        block & 0xFFFF,
    ]

    for i in reversed(range(rounds)):
        k = round_key(K, i)
        X = feistel_round(X, k)

    return (X[0] << 48) | (X[1] << 32) | (X[2] << 16) | X[3]


if __name__ == "__main__":
    key = random.getrandbits(64)
    block = 0x1234567890ABCDEF
    rounds = 8

    print(f"Key       = {key:016X}")
    print(f"Block     = {block:016X}")

    enc = encrypt(block, key, rounds)
    print(f"Encrypted = {enc:016X}")

    dec = decrypt(enc, key, rounds)
    print(f"Decrypted = {dec:016X}")
