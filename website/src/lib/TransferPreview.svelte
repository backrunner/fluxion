<script lang="ts">
  export let zh = false;
  let paused = false;
  let filter: 'all' | 'active' | 'done' = 'all';
  const samples = [
    { name: 'Design resources.zip', zhName: '设计素材.zip', protocol: 'HTTPS', size: '482 MB', progress: 68, done: false },
    { name: 'Open media collection', zhName: '开放媒体合集', protocol: 'BitTorrent', size: '1.2 GB', progress: 42, done: false },
    { name: 'Project backup.tar', zhName: '项目备份.tar', protocol: 'SFTP', size: '96 MB', progress: 100, done: true }
  ];
  $: shown = samples.filter(item => filter === 'all' || (filter === 'done' ? item.done : !item.done));
</script>

<div class="transfer-preview" aria-label={zh ? '下载工作区交互示意' : 'Interactive download workspace illustration'}>
  <div class="preview-titlebar"><span class="window-dots" aria-hidden="true"><i></i><i></i><i></i></span><span>Fluxion</span><span class="preview-local">{zh ? '在你的 Mac 上' : 'On your Mac'}</span></div>
  <div class="preview-body">
    <div class="preview-sidebar"><span class="preview-sidebar-title">{zh ? '资料库' : 'Library'}</span><span class="preview-selected"><span aria-hidden="true">↓</span> {zh ? '下载' : 'Downloads'} <b>3</b></span><div class="preview-meter"><span>{zh ? '传输速率' : 'Transfer speed'}</span><strong>{paused ? '0' : '12.4'} <small>MB/s</small></strong><svg viewBox="0 0 160 45" role="img" aria-label={zh ? '示意速率曲线' : 'Illustrative speed chart'}><path d={paused ? 'M0 42H160' : 'M0 40L9 37L18 39L27 25L36 28L45 16L54 24L63 20L72 31L81 22L90 26L99 15L108 19L117 10L126 14L135 7L144 16L153 10L160 13'} fill="none" stroke="currentColor" stroke-width="2"/></svg><span class="preview-meter-note">{zh ? '示例数据' : 'Sample data'}</span></div></div>
    <div class="preview-tasks">
      <div class="preview-heading"><h2>{zh ? '下载' : 'Downloads'}</h2><span class="preview-count">{zh ? '3 个任务' : '3 transfers'}</span></div>
      <div class="preview-filters" aria-label={zh ? '筛选示例任务' : 'Filter sample transfers'}>
        {#each ['all', 'active', 'done'] as key, index}
          <button type="button" class:current={filter === key} aria-pressed={filter === key} on:click={() => filter = key as typeof filter}>{(zh ? ['全部','进行中','已完成'] : ['All','In progress','Completed'])[index]}</button>
        {/each}
      </div>
      <div class="preview-task-list">
        {#each shown as item}
          <div class="preview-task">
            <div class="preview-file-icon" class:complete={item.done} aria-hidden="true">{item.done ? '✓' : '↓'}</div>
            <div class="preview-task-info"><div class="preview-task-name">{zh ? item.zhName : item.name}</div><div class="preview-task-meta"><span>{item.protocol}</span><span>{item.size}</span><span>{item.done ? (zh ? '已完成' : 'Completed') : paused ? (zh ? '已暂停' : 'Paused') : `${item.progress}%`}</span></div><div class="preview-progress" aria-hidden="true"><span style={`width:${item.progress}%`} class:paused={!item.done && paused} class:complete={item.done}></span></div></div>
          </div>
        {/each}
      </div>
      <div class="preview-bottom"><span><i class:idle={paused}></i>{paused ? (zh ? '已暂停示例任务' : 'Sample transfers paused') : (zh ? '一切尽在掌控' : 'Everything in view')}</span><button type="button" aria-pressed={paused} on:click={() => paused = !paused}>{paused ? (zh ? '继续示例' : 'Resume demo') : (zh ? '暂停示例' : 'Pause demo')}</button></div>
    </div>
  </div>
</div>
<p class="preview-caption">{zh ? '可交互界面示意 · 使用示例任务，并非应用截图' : 'Interactive illustration · sample transfers, not an app screenshot'}</p>
