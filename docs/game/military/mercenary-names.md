# Mercenary company and army names

A company name is localized when requested. Hiring separately assigns a cleaned copy to the newly created army.

## A company name and an army name are separate

The existing company name function `0x8ffcb0` takes `(company, std::string* out)`:

- With a template at company `+0x28`, it copies the template's explicit name at `+0x40` when nonempty. Otherwise it asks the localisation lookup `0x14b34a0` for the template's content key at `+0x20`.
- Without a template it returns an empty string.
- It moves the resulting string into `out`, destroys the emptied temporaries and returns `out`. It neither removes colors nor changes character case.

The hiring function `0x411c60` constructs the army, then gives it its name:

1. At `0x411cfc`, call the company name function.
2. At `0x411d0c`, pass that result as the source to `0x17046a0`, with a separate output string.
3. At `0x411d37`, copy the cleaned result into army `+0x148` with the ordinary string assignment `0x95110`.
4. Destroy both temporary strings, then associate the army with its company at army `+0x48` and continue constructing its regiments and hiring state.

The defect therefore occurs when the army is made, before it is drawn. The company getter can keep returning the correct name while the army's own name is already wrong. It is not caused by the font, an ellipsis or clipping a label to its box.

The common color remover is described in [text formatting](../interface/text-formatting.md); the Unicode repair and its raw-name evidence are [recorded separately](../../research/mercenary-company-names.md).
