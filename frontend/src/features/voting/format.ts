/** "+1.4" / "-0.3" / "0.0": always signed, one decimal. */
export const formatScore = (score: number) => `${score > 0 ? '+' : ''}${score.toFixed(1)}`
