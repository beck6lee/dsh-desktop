// Generate the visual-companion page showing whale girl icon candidates.
// Reads the previews, base64-embeds them, and writes the HTML into screen_dir.
const fs = require('node:fs');
const path = require('node:path');

const SCREEN_DIR = '/Users/beck.lee/Desktop/dsh-workspace/.superpowers/brainstorm/16538-1786756499/content';
const PREVIEW_DIR = '/Users/beck.lee/Desktop/dsh-workspace/assets/whale-girl/previews';

const candidates = [
  {
    id: 'improved-1',
    file: 'improved-1.jpg',
    name: '改进版 1（仓库推荐）',
    desc: '去伪影/去涂抹修复版，984×984 高清，DeepSeek 蓝配色，适合做主图标',
  },
  {
    id: 'improved-2',
    file: 'improved-2.jpg',
    name: '改进版 2',
    desc: '另一种修复风格，色彩更柔和',
  },
  {
    id: 'whale-girl-transparent',
    file: 'whale-girl-transparent.jpg',
    name: '透明底版本',
    desc: '910×941 透明底 PNG 转预览，自带去底，最省事的图标素材',
  },
  {
    id: 'original-whale-girl',
    file: 'original-whale-girl.jpg',
    name: '原图',
    desc: '社区流传的原始图（含背景），角色形象「溟月」',
  },
  {
    id: 'icon-preview',
    file: 'icon-preview.jpg',
    name: '图标效果预览',
    desc: '仓库作者生成的图标效果图（256×256），可直接看它在圆角方块中的样子',
  },
];

function b64(file) {
  return fs.readFileSync(path.join(PREVIEW_DIR, file)).toString('base64');
}

const cards = candidates
  .map(
    (c, i) => `<div class="card" data-choice="${c.id}" onclick="toggleSelect(this)">
    <div class="card-image" style="background:#f3f7ff;display:flex;align-items:center;justify-content:center;padding:18px">
      <img src="data:image/jpeg;base64,${b64(c.file)}" alt="${c.name}" style="max-width:100%;max-height:240px;border-radius:12px;box-shadow:0 4px 14px rgba(30,60,160,.15)">
    </div>
    <div class="card-body">
      <h3>${i + 1}. ${c.name}</h3>
      <p>${c.desc}</p>
    </div>
  </div>`,
  )
  .join('\n  ');

const html = `<style>
  .cards{grid-template-columns:repeat(auto-fit,minmax(260px,1fr));}
  .card{cursor:pointer;border:2px solid transparent;border-radius:16px;overflow:hidden;background:#fff;transition:border-color .15s, box-shadow .15s;}
  .card:hover{border-color:#4a90d9;box-shadow:0 6px 18px rgba(30,60,160,.12);}
  .card.selected{border-color:#1d5fc4;}
  .note{margin-top:14px;padding:12px 16px;background:#eef4ff;border-left:4px solid #1d5fc4;border-radius:8px;font-size:14px;line-height:1.7;}
</style>

<h2>选择鲸鱼娘图标素材</h2>
<p class="subtitle">来自社区仓库 deepseek-whale-girl-icon（角色「溟月」，CC BY-NC-SA 4.0，个人自用没问题）。点击你最喜欢的一张。</p>

<div class="cards">
  ${cards}
</div>

<div class="note">
  💡 <b>我的建议：</b>选 <b>1（改进版 1）</b> 或 <b>3（透明底版本）</b>——前者质量最高适合做 Dock 大图标，后者自带透明底适合直接生成 .icns。选定后我会用这张图生成 macOS 应用图标（.icns，含 16~1024 全尺寸），并保留原图在你的项目里。
</div>
`;

fs.writeFileSync(path.join(SCREEN_DIR, 'whale-girl-icons.html'), html);
console.log('written:', path.join(SCREEN_DIR, 'whale-girl-icons.html'));
