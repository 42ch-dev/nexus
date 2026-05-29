---
vars:
  topic: { type: string, required: true }
max_tokens: 500
---

# Revision Quality Check

Evaluate whether the revised draft is now of sufficient quality for the topic:

**{{preset.input.topic}}**

Check that:
1. All previously identified issues have been addressed
2. The revision maintains or improves upon the original's strengths
3. No new issues have been introduced
4. The overall quality is "good enough" for its intended purpose

If the revision passes quality review, respond with "GO". If further revision is needed, respond with "WAIT" and explain why.
