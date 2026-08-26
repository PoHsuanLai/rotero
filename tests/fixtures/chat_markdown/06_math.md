## Objective

The loss balances two terms, where $\alpha$ weights the second:

$$
L = \sum_i \| x_i - \hat{x}_i \|^2 + \alpha R(\theta)
$$

Setting $\alpha = 0$ recovers the unregularised case.
