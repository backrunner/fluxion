<script lang="ts">
  import type { SvedocsThemeContext } from 'svedocs/theme/types';
  import { resolveLocalizedHref } from 'svedocs/theme/headless';
  import TransferPreview from './TransferPreview.svelte';
  export let context: SvedocsThemeContext;
  $: zh = context.localeCode === 'zh';
  $: copy = zh ? {
    eyebrow: '为 MAC 原生打造', line1: '下载有序，', line2: '自在掌控。', description: '链接、种子、远程文件。把每一次传输放进同一个清晰的工作区，进度、细节与控制，始终在手边。', start: '了解使用方法', status: '查看发行状态', availability: '正在为首次稳定发行做准备', protocol: '不同来源，同样从容。', protocolLink: '了解协议支持',
    flowLabel: '为日常传输留出空间', flowTitle: '看得清楚，做得简单。', flowBody: '从添加任务到找到下载的文件，让常用操作自然顺手。',
    features: [
      { title: '所有进度，一目了然。', body: '搜索与筛选任务，在详情中查看传输状态。暂停、继续，或批量处理选中的任务。', mark: '01', href: '/docs/tasks', link: '管理下载任务' },
      { title: '下一次，从这里继续。', body: '保留任务历史，在服务器支持时续传部分文件。为单个任务或全局设置传输限速。', mark: '02', href: '/docs/downloading', link: '添加你的第一个下载' },
      { title: '每个协议，各有所长。', body: 'HTTP 与 HTTPS 分段下载，BitTorrent 选择文件，FTP 与 SFTP 访问远程文件。', mark: '03', href: '/docs/protocols', link: '选择适合的方式' }
    ],
    privateLabel: '本地管理，边界清晰', privateTitle: '你决定下载什么，\n也决定保存在哪里。', privateBody: '任务与设置保存在本机，敏感传输凭据存入 macOS 钥匙串。无需 Fluxion 账户，没有开发者分析追踪。', privateLink: '阅读隐私政策', privateNotes: ['由你选择下载目录', '任务凭据存入钥匙串', '诊断信息经过脱敏'], privateFootnote: '下载会连接你选择的服务器。BitTorrent 会向节点公开 IP 地址，并可能在下载期间上传数据片段。',
    guideLabel: '从这里开始', guideTitle: '少一点摸索，多一点答案。', guides: [{name:'第一个下载', body:'从一个链接，到本地文件。',href:'/docs/downloading'},{name:'文件与访问权限',body:'了解下载目录与沙盒访问。',href:'/docs/permissions'},{name:'传输遇到问题？',body:'续传、连接与主机密钥问题。',href:'/docs/troubleshooting'}]
  } : {
    eyebrow: 'BUILT NATIVELY FOR MAC', line1: 'Downloads,', line2: 'in good order.', description: 'Links, torrents, remote files. Bring every transfer into one clear workspace, with progress, details, and control always within reach.', start: 'Find your flow', status: 'Release status', availability: 'Preparing for the first stable release', protocol: 'Different sources. A familiar flow.', protocolLink: 'Explore protocol support',
    flowLabel: 'SPACE FOR YOUR EVERYDAY TRANSFERS', flowTitle: 'A clear view. A lighter workflow.', flowBody: 'From adding a link to finding your file, keep the everyday steps close at hand.',
    features: [
      { title: 'Every transfer, in view.', body: 'Search and filter your tasks. Open the details, pause or resume a transfer, or act on a selection together.', mark: '01', href: '/docs/tasks', link: 'Manage your transfers' },
      { title: 'Pick up where you left off.', body: 'Keep your task history and resume partial files when the server supports it. Set transfer limits per task or across the app.', mark: '02', href: '/docs/downloading', link: 'Add your first download' },
      { title: 'The right path for each file.', body: 'Segmented HTTP and HTTPS downloads, file selection for BitTorrent, and remote file access over FTP and SFTP.', mark: '03', href: '/docs/protocols', link: 'Explore the protocols' }
    ],
    privateLabel: 'LOCAL MANAGEMENT. CLEAR BOUNDARIES.', privateTitle: 'Choose your files.\nChoose their home.', privateBody: 'Tasks and settings stay on your Mac. Sensitive transfer credentials go in macOS Keychain. No Fluxion account, no developer analytics.', privateLink: 'Read the privacy policy', privateNotes: ['You choose the download folder', 'Task credentials in Keychain', 'Redacted diagnostics'], privateFootnote: 'Transfers connect to the servers you choose. BitTorrent exposes your IP address to peers and may upload pieces during downloads.',
    guideLabel: 'A GOOD PLACE TO BEGIN', guideTitle: 'Less guesswork. More answers.', guides: [{name:'Your first download',body:'From a link to a local file.',href:'/docs/downloading'},{name:'Files & permissions',body:'Download folders and sandbox access.',href:'/docs/permissions'},{name:'A transfer not moving?',body:'Resume, connections, and host-key questions.',href:'/docs/troubleshooting'}]
  };
  const href = (path: string) => resolveLocalizedHref(path, context);
</script>

<div class="flux-landing">
  <section class="flux-hero" aria-labelledby="hero-title">
    <div class="flux-hero-copy"><span class="flux-eyebrow"><i></i>{copy.eyebrow}</span><h1 id="hero-title">{copy.line1}<br/><span>{copy.line2}</span></h1><p>{copy.description}</p><div class="flux-actions"><a class="flux-button flux-primary" href={href('/docs/downloading')}>{copy.start}<span aria-hidden="true">↗</span></a><a class="flux-button flux-secondary" href={href('/download')}>{copy.status}<span aria-hidden="true">→</span></a></div><p class="flux-availability">{copy.availability}</p></div>
    <div class="flux-stage"><div class="flux-stage-orbit" aria-hidden="true"></div><TransferPreview {zh}/></div>
  </section>
  <section class="flux-protocols" aria-label={copy.protocol}><p>{copy.protocol}</p><div>{#each ['HTTP','HTTPS','BitTorrent','FTP','SFTP'] as protocol}<span>{protocol}</span>{/each}</div><a href={href('/docs/protocols')}>{copy.protocolLink}<span aria-hidden="true">↗</span></a></section>
  <section class="flux-flow"><div class="flux-section-heading"><div><span class="flux-eyebrow">{copy.flowLabel}</span><h2>{copy.flowTitle}</h2></div><p>{copy.flowBody}</p></div><div class="flux-feature-grid">{#each copy.features as feature}<article><span class="flux-feature-number">{feature.mark} /</span><h3>{feature.title}</h3><p>{feature.body}</p><a href={href(feature.href)}>{feature.link}<span aria-hidden="true">→</span></a></article>{/each}</div></section>
  <section class="flux-private"><div><span class="flux-eyebrow">{copy.privateLabel}</span><h2>{copy.privateTitle}</h2><p>{copy.privateBody}</p><a href={href('/privacy')}>{copy.privateLink}<span aria-hidden="true">↗</span></a></div><div class="flux-private-detail"><div class="flux-private-mark"><img src="/app-icon.png" alt="" width="52" height="52"/><span>Fluxion<small>{zh ? '你的 Mac，你的工作区。' : 'Your Mac. Your workspace.'}</small></span></div><ul>{#each copy.privateNotes as note}<li><span aria-hidden="true">✓</span>{note}</li>{/each}</ul><p>{copy.privateFootnote}</p></div></section>
  <section class="flux-guides"><div><span class="flux-eyebrow">{copy.guideLabel}</span><h2>{copy.guideTitle}</h2></div><div class="flux-guide-list">{#each copy.guides as guide, index}<a href={href(guide.href)}><span class="flux-guide-number">0{index+1}</span><div><h3>{guide.name}</h3><p>{guide.body}</p></div><span class="flux-guide-arrow" aria-hidden="true">↗</span></a>{/each}</div></section>
</div>
