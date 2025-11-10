# Pure Python node - should be browser compatible

import json
import sys

class SimpleCalculator:
    def __init__(self, operation="add"):
        self.operation = operation
    
    def process(self, a, b):
        if self.operation == "add":
            return a + b
        elif self.operation == "multiply":
            return a * b
        else:
            return 0

if __name__ == "__main__":
    calc = SimpleCalculator()
    print(calc.process(5, 3))
