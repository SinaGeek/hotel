from django.shortcuts import render, get_object_or_404, redirect
from django.contrib.auth.decorators import login_required
from .models import InventoryItem, InventoryTransaction, InventoryColumn
from django.db.models import F

@login_required
def inventory_list_view(request):
    sort_by = request.GET.get('sort', 'name')
    order = request.GET.get('order', 'asc')
    
    if order == 'desc':
        items = InventoryItem.objects.all().order_by(F(sort_by).desc(nulls_last=True))
    else:
        items = InventoryItem.objects.all().order_by(F(sort_by).asc(nulls_last=True))
    
    if request.method == 'POST':
        name = request.POST.get('name')
        unit = request.POST.get('unit', 'pcs')
        qty = request.POST.get('quantity', 0)
        min_l = request.POST.get('min_level', 5)
        hotel_id = request.user.hotel_id or 1
        
        InventoryItem.objects.create(
            name=name, unit=unit, 
            quantity=qty, min_level=min_l,
            hotel_id=hotel_id
        )
        return redirect('inventory_list')

    columns = InventoryColumn.objects.filter(is_active=True)
    
    return render(request, 'inventory/list.html', {
        'items': items,
        'columns': columns,
        'current_sort': sort_by,
        'current_order': order
    })

@login_required
def inventory_detail_view(request, item_id):
    item = get_object_or_404(InventoryItem, id=item_id)
    
    if request.method == 'POST':
        action = request.POST.get('action')
        qty = float(request.POST.get('quantity', 0))
        notes = request.POST.get('notes', '')
        
        if action in ['ISSUE', 'OUT']:
            item.quantity -= qty
            t_type = 'ISSUE'
        elif action in ['RETURN', 'IN']:
            item.quantity += qty
            t_type = 'RETURN'
        elif action == 'REQUEST':
            # Request might not change quantity immediately?
            t_type = 'REQUEST'
        elif action == 'BUY':
            item.quantity += qty
            item.last_buy_price = float(request.POST.get('price', item.last_buy_price))
            t_type = 'BUY'
            
        item.save()
        InventoryTransaction.objects.create(
            item=item,
            transaction_type=t_type,
            quantity=qty,
            user=request.user,
            notes=notes
        )
        return redirect('inventory_detail', item_id=item.id)
        
    transactions = item.transactions.all().order_by('-timestamp').select_related('user')
    return render(request, 'inventory/detail.html', {
        'item': item,
        'transactions': transactions
    })
