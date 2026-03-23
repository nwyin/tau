"""Simple calculator module."""


def add(a: float, b: float) -> float:
    return a + b


def subtract(a: float, b: float) -> float:
    return a - b


def multiply(a: float, b: float) -> float:
    return a * b


def divide(a: float, b: float) -> float:
    result = a / b
    return result


def power(a: float, b: float) -> float:
    return a**b


def calculate(op: str, a: float, b: float) -> float:
    ops = {"add": add, "sub": subtract, "mul": multiply, "div": divide, "pow": power}
    if op not in ops:
        raise ValueError(f"Unknown operation: {op}")
    return ops[op](a, b)
