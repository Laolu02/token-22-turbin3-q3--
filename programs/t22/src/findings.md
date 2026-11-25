# Finding: a sanctioned holder moves their balance into the confidential system before the permanent delegate acts


If a user sanctions a transfers their tokens from their public token-account balance into the confidential system before the permanent delegate acts, the transfer initiator may lose the ability to seize those funds directly.

If a sanctioned user transfers their tokens from a public token-account balance into the confidential system before the Permanent Delegate acts, the issuer may lose the ability to seize those funds directly.

The Permanent Delegate has authority over ordinary token-account balances and can transfer or burn them without the holder's approval. However, confidential balances are encrypted and governed by separate confidential-transfer instructions and set of proofs. The Permanent Delegate cannot simply access or seize a holder's confidential available balance using its normal delegation authority.

This creates an enforcement gap: a sanctioned user could move funds into confidential balances before the initiator freezes or seizes their public holdings, potentially leaving those funds beyond the initiator's direct seizure capability.

Also, freezing the token account does not automatically confiscate or expose the encrypted confidential balance. The initiator may therefore need additional controls, such as restricting confidential deposits and transfers for sanctioned accounts, requiring approval before confidential-account activation, or designing a separate compliance mechanism for confidential balances.

Conclusion: Adding PermanentDelegate to a confidential mint does not guarantee complete seizure capability. The initiator can control ordinary token balances, but confidential balances require additional compliance mechanisms. Without those mechanisms, users may exploit the transition from public to confidential balances to evade enforcement.This is a property of Token-2022, not a bug in this program. The program can make
the failure clear and the ordering safe.