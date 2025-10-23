"""
Simple math nodes for examples and testing.
These nodes perform basic mathematical operations on numeric data.
"""

from typing import Union, List
from ..core.node import Node


class MultiplyNode(Node):
    """Node that multiplies input values by a factor."""

    def __init__(self, factor: float = 2.0, name: str = "multiply"):
        super().__init__(name=name)
        self.factor = factor

    def process(self, data: Union[int, float, List]) -> Union[int, float, List]:
        """Multiply input by the factor."""
        if isinstance(data, list):
            return [x * self.factor for x in data]
        return data * self.factor


class AddNode(Node):
    """Node that adds a constant to input values."""

    def __init__(self, addend: float = 0.0, name: str = "add"):
        super().__init__(name=name)
        self.addend = addend

    def process(self, data: Union[int, float, List]) -> Union[int, float, List]:
        """Add the addend to input."""
        if isinstance(data, list):
            return [x + self.addend for x in data]
        return data + self.addend


__all__ = ["MultiplyNode", "AddNode"]
