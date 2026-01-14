"""
Test classes for remote proxy examples.
"""


class Counter:
    def __init__(self, initial_value=0):
        self.value = initial_value
    
    def increment(self):
        self.value += 1
        return self.value
    
    def get_value(self):
        return self.value
    
    def add(self, amount):
        self.value += amount
        return self.value


class DataProcessor:
    def __init__(self, mode="fast"):
        self.mode = mode
        self.processed_count = 0
    
    def process(self, data):
        # Simulate expensive processing
        result = f"Processed '{data}' in {self.mode} mode"
        self.processed_count += 1
        return result
    
    def get_stats(self):
        return {
            "mode": self.mode,
            "processed_count": self.processed_count
        }