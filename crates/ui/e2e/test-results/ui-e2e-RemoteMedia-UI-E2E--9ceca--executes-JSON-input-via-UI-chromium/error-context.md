# Page snapshot

```yaml
- generic [ref=e3]:
  - banner [ref=e4]:
    - heading "RemoteMedia" [level=1] [ref=e5]
    - generic [ref=e6]:
      - generic [ref=e8]: Connected
      - generic [ref=e9]: grpc
      - generic [ref=e10]: 0 sessions
  - navigation [ref=e11]:
    - button "Pipeline" [ref=e12] [cursor=pointer]
    - button "Manifest" [ref=e13] [cursor=pointer]
  - main [ref=e14]:
    - generic [ref=e15]:
      - heading "Pipeline Execution" [level=2] [ref=e16]
      - generic [ref=e17]:
        - generic [ref=e18]: "Input type:"
        - combobox [ref=e19] [cursor=pointer]:
          - option "Text"
          - option "JSON" [selected]
          - option "Audio (record)"
      - 'textbox "{\"key\": \"value\"}" [ref=e20]': "{\"key\": \"value\", \"num\": 42}"
      - button "Execute" [ref=e22] [cursor=pointer]
      - generic [ref=e23]: Unexpected token 'F', "Failed to "... is not valid JSON
```