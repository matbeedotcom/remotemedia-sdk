# Python node with C-extension (numpy) - requires native execution

import numpy as np
import json

class NumpyProcessor:
    def __init__(self, factor=2.0):
        self.factor = factor
    
    def process(self, data):
        # Use numpy for processing
        array = np.array(data)
        result = array * self.factor
        return result.tolist()

if __name__ == "__main__":
    proc = NumpyProcessor()
    print(proc.process([1, 2, 3, 4, 5]))
