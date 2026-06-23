import { defineConfig } from 'vitepress'

export default defineConfig({
  title: 'Kioku',
  description: 'AI Meeting Intelligence Platform',
  base: '/',
  head: [
    ['link', { rel: 'icon', href: '/assets/favicon.ico' }],
    ['link', { rel: 'preconnect', href: 'https://fonts.googleapis.com' }],
    ['link', { rel: 'preconnect', href: 'https://fonts.gstatic.com', crossorigin: '' }],
    ['link', { rel: 'stylesheet', href: 'https://fonts.googleapis.com/css2?family=Fraunces:opsz,wght@9..144,300;9..144,400;9..144,500;9..144,600;9..144,700&family=DM+Sans:ital,wght@0,300;0,400;0,500;0,600;0,700;1,400&family=JetBrains+Mono:wght@400;500&display=swap' }],
  ],
  themeConfig: {
    logo: {
      dark: '/assets/logo-dark.svg',
      light: '/assets/logo-light.svg',
    },
    nav: [
      { text: 'Docs', link: '/' },
      { text: 'API', link: '/api/authentication' },
    ],
    sidebar: {
      '/': [
        {
          text: 'Start Here',
          items: [
            { text: 'Introduction', link: '/' },
            { text: 'Quick Start', link: '/quickstart' },
            { text: 'Core Concepts', link: '/concepts' },
          ]
        },
        {
          text: 'Deploy',
          items: [
            { text: 'Docker Compose', link: '/deployment/docker' },
            { text: 'RunPod', link: '/deployment/runpod' },
            { text: 'Environment Variables', link: '/deployment/environment-variables' },
          ]
        },
        {
          text: 'Architecture',
          items: [
            { text: 'Overview', link: '/architecture' },
            { text: 'Hivemind', link: '/architecture/hivemind' },
            { text: 'CLI', link: '/architecture/cli' },
            { text: 'Vexa Integration', link: '/architecture/vexa' },
          ]
        },
        {
          text: 'Knowledge',
          items: [
            { text: 'Overview', link: '/knowledge/overview' },
            { text: 'Search', link: '/knowledge/search' },
            { text: 'Documents', link: '/knowledge/documents' },
            { text: 'Meetings', link: '/knowledge/meetings' },
          ]
        },
        {
          text: 'MCP',
          items: [
            { text: 'Overview', link: '/mcp/overview' },
            { text: 'Tools', link: '/mcp/tools' },
          ]
        },
        {
          text: 'Contributing',
          items: [
            { text: 'Contributing', link: '/contributing' },
            { text: 'Testing', link: '/testing' },
          ]
        },
      ],
      '/api/': [
        {
          text: 'API Reference',
          items: [
            { text: 'Authentication', link: '/api/authentication' },
            { text: 'Sessions', link: '/api/sessions' },
            { text: 'Knowledge', link: '/api/knowledge' },
            { text: 'Meetings', link: '/api/meetings' },
            { text: 'Company', link: '/api/company' },
            { text: 'Usage', link: '/api/usage' },
            { text: 'Vexa', link: '/api/vexa' },
          ]
        }
      ]
    },
    socialLinks: [
      { icon: 'github', link: 'https://github.com/kioku/kioku' }
    ],
    search: {
      provider: 'local'
    }
  }
})