# Plan Reports

This directory contains supplementary reports for plan execution, including:

- **Architecture Reviews**: Technical reviews and recommendations
- **QC Reports**: Quality control findings from multiple reviewers
- **Consolidated Decisions**: Merged review conclusions

## Directory Structure

```
reports/
├── 2025-04-05-domain-models/
│   ├── 2025-04-05-domain-models-review.md           # Architecture review
│   ├── 2025-04-05-domain-models-qc1.md              # QC Specialist #1 report
│   ├── 2025-04-05-domain-models-qc2.md              # QC Specialist #2 report
│   ├── 2025-04-05-domain-models-qc3.md              # QC Specialist #3 report
│   └── 2025-04-05-domain-models-qc-consolidated.md  # Consolidated QC decision
└── README.md
```

## Usage

- **Reports are read-only historical records**
- **Residual findings are tracked in `../status.json`** under `metadata.residual_findings`
- For active plans, see the main plan file (e.g., `../2025-04-05-domain-models.md`)

## Naming Convention

- `<plan-id>-review.md`: Architecture review reports
- `<plan-id>-qc<#>.md`: Individual QC reports
- `<plan-id>-qc-consolidated.md`: Consolidated QC decisions

## Finding Residual Issues

All residual findings from QC reviews are tracked in `../status.json`:

```json
{
  "metadata": {
    "residual_findings": {
      "2025-04-05-domain-models": [
        {
          "id": "R1",
          "title": "Finding title",
          "severity": "medium|low",
          "decision": "defer|accept",
          "owner": "@fullstack-dev",
          "target": "When to address"
        }
      ]
    }
  }
}
```

---

**Maintainer**: @project-manager  
**Last Updated**: 2026-04-06
