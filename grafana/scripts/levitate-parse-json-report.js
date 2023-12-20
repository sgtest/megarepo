const fs = require('fs');

const data = JSON.parse(fs.readFileSync('data.json', 'utf8'));

const stripAnsi = (string) => string.replace(/\u001b\[.*?m/g, '');

const printSection = (title, items) => {
  let output = `<h4>${title}</h4>`;
  items.forEach((item) => {
    const language = item.declaration ? 'typescript' : 'diff';
    const code = item.declaration ? item.declaration : stripAnsi(item.diff);

    output += `<b>${item.name}</b><br>`;
    output += `<sub>${item.location}</sub><br>`;
    output += `<pre lang="${language}">${code}</pre><br>`;
  });
  return output;
};

let markdown = '';

if (data.removals.length > 0) {
  markdown += printSection('Removals', data.removals);
}
if (data.changes.length > 0) {
  markdown += printSection('Changes', data.changes);
}

console.log(markdown);
