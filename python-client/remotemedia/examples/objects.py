"""
Example objects that can be used with remote proxy.
"""

import numpy as np


class SimpleCounter:
    def __init__(self):
        self.count = 0
    
    def add(self, n):
        self.count += n
        return self.count


class StringProcessor:
    def reverse(self, text):
        return text[::-1]
    
    def uppercase(self, text):
        return text.upper()
    
    def process(self, text, reverse=False, uppercase=False):
        if reverse:
            text = self.reverse(text)
        if uppercase:
            text = self.uppercase(text)
        return text


class MathOperations:
    def __init__(self):
        self.history = []
    
    def calculate(self, expression):
        result = eval(expression)  # Simple eval for demo
        self.history.append(f"{expression} = {result}")
        return result
    
    def get_history(self):
        return self.history
    
    def matrix_multiply(self, a, b):
        # Works with numpy arrays too
        return np.dot(a, b)


class TodoList:
    def __init__(self):
        self.todos = []
        self.completed = []
    
    def add_todo(self, task):
        self.todos.append(task)
        return f"Added: {task}"
    
    def complete_todo(self, index):
        if 0 <= index < len(self.todos):
            task = self.todos.pop(index)
            self.completed.append(task)
            return f"Completed: {task}"
        return "Invalid index"
    
    def get_status(self):
        return {
            "pending": self.todos,
            "completed": self.completed,
            "total": len(self.todos) + len(self.completed)
        }


class Calculator:
    """A simple calculator that maintains operation history."""
    
    def __init__(self):
        self.result = 0
        self.operations = []
    
    def add(self, x, y=None):
        if y is None:
            self.result += x
            self.operations.append(f"Added {x}, result: {self.result}")
            return self.result
        else:
            result = x + y
            self.operations.append(f"{x} + {y} = {result}")
            return result
    
    def multiply(self, x, y):
        result = x * y
        self.operations.append(f"{x} * {y} = {result}")
        return result
    
    def reset(self):
        self.result = 0
        self.operations.append("Reset to 0")
        return "Calculator reset"
    
    def history(self):
        return self.operations