export const ptBR = {
  general: {
    loading: "Carregando…",
  },
  setup: {
    description: "Disponibilize localhost:3000 em app.yourdomain.com pela Cloudflare.",
    readGuide: "Primeira vez aqui? Leia o guia",
    connectAccount: "Conecte sua conta da Cloudflare",
    createToken: {
      title: "Crie um token na Cloudflare",
      button: "Criar token ↗",
      descriptionBeforePermission:
        "O formulário abre com DNS, Zone e Account Settings preenchidos. Adicione mais uma permissão, ",
      permission: "Account → Cloudflare Tunnel → Edit",
      descriptionAfterPermission:
        ", depois selecione Continue to summary → Create Token.",
    },
    pasteToken: {
      title: "Cole aqui",
      placeholder: "Cole o token",
      checking: "Verificando…",
      connect: "Conectar",
    },
    permissions: {
      title: "Por que essas permissões?",
      tunnel: {
        name: "Account · Cloudflare Tunnel · Edit",
        description: " — criar e executar o túnel deste computador",
      },
      account: {
        name: "Account · Account Settings · Read",
        description: " — encontrar sua conta da Cloudflare",
      },
      dns: {
        name: "Zone · DNS · Edit",
        description: " — apontar seus subdomínios para o túnel",
      },
      zone: {
        name: "Zone · Zone · Edit",
        description: " — adicionar domínios pelo TunnelPup, incluindo os gratuitos",
      },
    },
    tokenStorage:
      "O token é armazenado no gerenciador de credenciais do sistema, nunca em um arquivo.",
    guideTitle: "Guia",
  },
};
