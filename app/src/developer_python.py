"""Trusted, deliberately small Python execution language for Lucille 4.1.
Inputs are approved source snapshots, never imports from disk. Not a general Python sandbox.
"""
import ast
import builtins
import json
import math
import sys
import types
import unittest

SAFE_BUILTINS = {name:getattr(builtins,name) for name in ('abs','all','any','bool','dict','enumerate','float','int','len','list','max','min','print','range','round','set','str','sum','tuple','zip','ValueError','AssertionError','RuntimeError')}
SAFE_BUILTINS['__build_class__']=builtins.__build_class__
ASSERTS={'assertEqual','assertNotEqual','assertTrue','assertFalse','assertIn','assertNotIn','assertAlmostEqual','assertIsNone','assertIsNotNone','assertGreater','assertLess'}
MATH={'sqrt','floor','ceil','sin','cos','tan','log','exp','fabs','isclose','pi','e'}
NODES={getattr(ast,n) for n in ('Module','FunctionDef','arguments','arg','Return','BinOp','Add','Sub','Mult','Div','FloorDiv','Mod','Pow','UnaryOp','UAdd','USub','Not','Constant','Name','Load','Store','Expr','Call','Import','ImportFrom','alias','ClassDef','Attribute','Assert','Compare','Eq','NotEq','Lt','LtE','Gt','GtE','Is','IsNot','In','NotIn','If','IfExp','Raise','List','Tuple','Dict','Set','For','While','Break','Continue','Assign','AugAssign','keyword','Pass','BoolOp','And','Or','Subscript','Slice','JoinedStr','FormattedValue')}

def validate(source,modules):
    tree=ast.parse(source)
    for n in ast.walk(tree):
        if type(n) not in NODES:raise ValueError('Unsupported Python syntax: '+type(n).__name__)
        if isinstance(n,(ast.Name,ast.arg)):
            name=n.id if isinstance(n,ast.Name) else n.arg
            if name.startswith('_') and name!='__name__':raise ValueError('Private/reflection names are unavailable')
        if isinstance(n,(ast.FunctionDef,ast.ClassDef)):
            if n.name.startswith('_') or n.decorator_list:raise ValueError('Decorators/private definitions are unavailable')
        if isinstance(n,ast.ClassDef):
            if n.keywords or len(n.bases)!=1 or ast.unparse(n.bases[0])!='unittest.TestCase':raise ValueError('Only unittest.TestCase classes are available')
        if isinstance(n,ast.Attribute):
            owner=n.value.id if isinstance(n.value,ast.Name) else ''
            if n.attr.startswith('_') or not ((owner=='self' and n.attr in ASSERTS) or (owner=='unittest' and n.attr in {'main','TestCase'}) or (owner=='math' and n.attr in MATH) or (owner in modules and not n.attr.startswith('_'))):raise ValueError('Unsupported attribute access')
        if isinstance(n,ast.Import):
            if any(a.name not in {'unittest','math'}|set(modules) or a.asname for a in n.names):raise ValueError('Import is outside approved modules')
        if isinstance(n,ast.ImportFrom):
            if n.level or n.module not in modules or any(a.name.startswith('_') or a.asname for a in n.names):raise ValueError('Import is outside approved modules')
    return tree

def execute(request):
    sources=request['sources'];entry=request['entry']
    if not isinstance(sources,dict) or len(sources)>128 or entry not in sources:raise ValueError('Invalid approved source manifest')
    modules={name[:-3] for name in sources if name.endswith('.py') and name[:-3].isidentifier()}
    if set(modules)&{'sys','os','types','builtins','unittest','math'}:raise ValueError('Protected module name')
    trees={name:validate(source,modules) for name,source in sources.items()}
    loaded={};loading=set()
    def load(name):
        if name in loaded:return loaded[name]
        if name in loading:raise ValueError('Cyclic imports unsupported in foundation runner')
        loading.add(name)
        filename=name+'.py'
        module=types.ModuleType(name)
        module.__dict__.update({'__builtins__':safe,'__file__':filename})
        exec(compile(trees[filename],filename,'exec'),module.__dict__)
        loading.remove(name);loaded[name]=module
        return module
    def importer(name,globals=None,locals=None,fromlist=(),level=0):
        if level:raise ImportError('Relative import unavailable')
        if name=='unittest':return public_unittest
        if name=='math':return public_math
        if name not in modules:raise ImportError('Module not approved')
        return load(name)
    safe=dict(SAFE_BUILTINS,__import__=importer)
    # Never expose unittest.main's dynamic module/test-name importer to source.
    public_unittest=types.SimpleNamespace(TestCase=unittest.TestCase,
        main=lambda:unittest.main(module='__main__',argv=[entry]))
    public_math=types.SimpleNamespace(**{name:getattr(math,name) for name in MATH})
    module=types.ModuleType('__main__');module.__dict__.update({'__builtins__':safe,'__file__':entry});sys.modules['__main__']=module
    sys.argv=[entry]
    exec(compile(trees[entry],entry,'exec'),module.__dict__)

if __name__=='__main__':
    try:execute(json.load(sys.stdin))
    except SystemExit:raise
    except BaseException as error:
        # A failed test is printed by unittest; bootstrap errors never echo source payloads.
        print('Restricted runner refused or failed: '+type(error).__name__,file=sys.stderr)
        sys.exit(2)
