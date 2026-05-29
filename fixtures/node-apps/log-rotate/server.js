let index = 0;

setInterval(() => {
  console.log(`rotate-line-${index}-${"x".repeat(80)}`);
  index += 1;
}, 20);
