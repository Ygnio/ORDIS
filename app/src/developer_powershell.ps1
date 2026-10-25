# Trusted runner: no profile, no dynamic command names, member invocation or external commands.
$ErrorActionPreference='Stop'
[Console]::InputEncoding=[Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false)
try {
  $request=[Console]::In.ReadToEnd() | ConvertFrom-Json
  $tokens=$null;$errors=$null
  $tree=[System.Management.Automation.Language.Parser]::ParseInput([string]$request.source,[ref]$tokens,[ref]$errors)
  if($errors.Count){throw 'Invalid syntax'}
  $allowed=@('ScriptBlockAst','NamedBlockAst','StatementBlockAst','PipelineAst','CommandAst','CommandParameterAst','StringConstantExpressionAst','ConstantExpressionAst','CommandExpressionAst','ExitStatementAst','ThrowStatementAst')
  foreach($node in $tree.FindAll({param($n) $true},$true)) {
    if($node.GetType().Name -notin $allowed){throw 'Unsupported syntax'}
    if($node -is [System.Management.Automation.Language.PipelineAst] -and $node.PipelineElements.Count -ne 1){throw 'Pipelines unavailable'}
    if($node -is [System.Management.Automation.Language.CommandAst]) {
      if($node.InvocationOperator -ne 'Unknown' -or $node.Redirections.Count -or $node.GetCommandName() -notin @('Write-Output','Start-Sleep')){throw 'Command unavailable'}
      foreach($element in $node.CommandElements){if($element -is [System.Management.Automation.Language.CommandParameterAst] -and $element.ParameterName -notin @('Seconds','Milliseconds')){throw 'Parameter unavailable'}}
    }
  }
  & ([ScriptBlock]::Create([string]$request.source))
  exit 0
} catch { [Console]::Error.WriteLine('Restricted PowerShell runner refused or failed.');exit 2 }
