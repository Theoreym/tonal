# Domaines Google

`google-domains.txt` vient de https://www.google.com/supported_domains (187 domaines, récupération le 11 novembre 2026).

La liste est embarquée : aucune requête supplémentaire n'est nécessaire pendant la connexion. Seuls le domaine exact et ses préfixes `accounts.` / `www.` sont reconnus. Hors des destinations globales déjà autorisées, seules les routes exactes de transfert `SetSID` et `CheckCookie` sont permises, en HTTPS sans identifiants d'URL ni port personnalisé. Les autres sous-domaines et les noms ressemblants restent refusés.
