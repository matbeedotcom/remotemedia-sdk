"""
Test classes with different method types for RemoteProxyClient.
"""

import asyncio
from typing import AsyncGenerator, Generator


class SyncMethods:
    """Class with only synchronous methods."""
    
    def __init__(self):
        self.counter = 0
    
    def add(self, x, y):
        """Simple sync method."""
        return x + y
    
    def increment(self):
        """Sync method that modifies state."""
        self.counter += 1
        return self.counter
    
    def get_counter(self):
        """Sync method that returns state."""
        return self.counter


class AsyncMethods:
    """Class with async methods."""
    
    def __init__(self):
        self.data = []
    
    async def async_add(self, x, y):
        """Async method with computation."""
        await asyncio.sleep(0.1)  # Simulate async work
        return x + y
    
    async def fetch_data(self, item):
        """Async method that modifies state."""
        await asyncio.sleep(0.05)
        self.data.append(item)
        return f"Fetched: {item}"
    
    async def get_all_data(self):
        """Async method that returns state."""
        await asyncio.sleep(0.01)
        return self.data


class GeneratorMethods:
    """Class with generator methods."""
    
    def __init__(self):
        self.max_value = 10
    
    def count_up_to(self, n):
        """Generator that yields values."""
        for i in range(min(n, self.max_value)):
            yield i
    
    def fibonacci(self, n):
        """Generator for Fibonacci sequence."""
        a, b = 0, 1
        for _ in range(n):
            yield a
            a, b = b, a + b
    
    def get_squares(self):
        """Generator using instance state."""
        for i in range(self.max_value):
            yield i * i


class AsyncGeneratorMethods:
    """Class with async generator methods."""
    
    def __init__(self):
        self.delay = 0.1
    
    async def async_count(self, n):
        """Async generator that yields values."""
        for i in range(n):
            await asyncio.sleep(self.delay)
            yield i
    
    async def stream_data(self, items):
        """Async generator that processes items."""
        for item in items:
            await asyncio.sleep(self.delay)
            yield f"Processed: {item}"
    
    async def infinite_stream(self):
        """Infinite async generator."""
        count = 0
        while True:
            await asyncio.sleep(self.delay)
            yield count
            count += 1


class MixedMethods:
    """Class with all types of methods."""
    
    def __init__(self, name="Mixed"):
        self.name = name
        self.history = []
    
    # Sync methods
    def sync_method(self, value):
        """Regular sync method."""
        self.history.append(f"sync: {value}")
        return f"{self.name} processed {value}"
    
    # Async methods
    async def async_method(self, value):
        """Regular async method."""
        await asyncio.sleep(0.1)
        self.history.append(f"async: {value}")
        return f"{self.name} async processed {value}"
    
    # Generator
    def generate_items(self, count):
        """Sync generator."""
        for i in range(count):
            self.history.append(f"generated: {i}")
            yield f"{self.name}-item-{i}"
    
    # Async generator
    async def async_generate(self, count):
        """Async generator."""
        for i in range(count):
            await asyncio.sleep(0.05)
            self.history.append(f"async_generated: {i}")
            yield f"{self.name}-async-item-{i}"
    
    # Property
    @property
    def status(self):
        """Property access."""
        return {
            "name": self.name,
            "history_count": len(self.history)
        }
    
    # Static method
    @staticmethod
    def static_add(x, y):
        """Static method."""
        return x + y
    
    # Class method
    @classmethod
    def create_named(cls, name):
        """Class method."""
        return cls(name=name)
    
    def get_history(self):
        """Get the history."""
        return self.history


class SpecialMethods:
    """Class with special Python methods."""
    
    def __init__(self, value=0):
        self.value = value
    
    def __str__(self):
        return f"SpecialMethods(value={self.value})"
    
    def __repr__(self):
        return f"SpecialMethods({self.value})"
    
    def __add__(self, other):
        return SpecialMethods(self.value + other)
    
    def __len__(self):
        return self.value
    
    def __getitem__(self, key):
        return f"item_{key}_{self.value}"
    
    def __call__(self, x):
        """Make the object callable."""
        return self.value * x
    
    def increment(self):
        """Regular method to test."""
        self.value += 1
        return self.value