from django.shortcuts import render
from django.contrib.auth.decorators import login_required
from .models import Room, RoomType
from apps.reservations.models import Reservation
import datetime

@login_required
def room_overview_view(request):
    user = request.user
    if request.method == 'POST' and user.is_superuser:
        action = request.POST.get('action')
        from django.http import JsonResponse
        hotel_id = user.hotel_id or 1
        
        try:
            if action == 'add_room_type':
                name_en = request.POST.get('name_en')
                name_tr = request.POST.get('name_tr')
                name_fa = request.POST.get('name_fa')
                if not name_en or not name_tr or not name_fa:
                    return JsonResponse({'error': 'ارائه هر سه زبان الزامی است.'}, status=400)
                RoomType.objects.create(name_en=name_en, name_tr=name_tr, name_fa=name_fa, hotel_id=hotel_id)
            elif action == 'add_rooms':
                type_id = request.POST.get('type_id')
                target_count = int(request.POST.get('count', 1))
                current_num = int(request.POST.get('start_num', 101))
                floor = int(request.POST.get('floor', 1))
                room_type = RoomType.objects.get(id=type_id)
                
                created_count = 0
                while created_count < target_count:
                    num_str = str(current_num)
                    if not Room.objects.filter(number=num_str, hotel_id=hotel_id).exists():
                        Room.objects.create(number=num_str, room_type=room_type, floor=floor, hotel_id=hotel_id)
                        created_count += 1
                    current_num += 1
                return JsonResponse({'success': True, 'created': created_count})
            elif action == 'edit_room':
                room_id = request.POST.get('room_id')
                number = request.POST.get('number')
                type_id = request.POST.get('type_id')
                floor = request.POST.get('floor')
                Room.objects.filter(id=room_id, hotel_id=hotel_id).update(
                    number=number,
                    room_type_id=type_id,
                    floor=floor
                )
            elif action == 'delete_room_type':
                type_id = request.POST.get('type_id')
                RoomType.objects.filter(id=type_id).delete()
            elif action == 'delete_room':
                room_id = request.POST.get('room_id')
                Room.objects.filter(id=room_id).delete()
        except Exception as e:
            return JsonResponse({'error': str(e)}, status=500)

        if request.headers.get('x-requested-with') == 'XMLHttpRequest':
            return JsonResponse({'success': True})
            
    # Persist preferences in session
    view_type = request.GET.get('view') or request.session.get('view_type', 'large')
    group_by = request.GET.get('group') or request.session.get('group_by', 'type')
    range_type = request.GET.get('range') or request.session.get('range_type', 'weekly')
    date_str = request.GET.get('date')
    
    request.session['view_type'] = view_type
    request.session['group_by'] = group_by
    request.session['range_type'] = range_type
    
    # Calculate Date Range
    if date_str:
        try:
            current_date = datetime.datetime.strptime(date_str, '%Y-%m-%d').date()
        except:
            current_date = datetime.date.today()
    else:
        current_date = datetime.date.today()

    if range_type == 'weekly':
        days_count = 7
        delta = datetime.timedelta(days=7)
    elif range_type == 'monthly':
        days_count = 30
        delta = datetime.timedelta(days=30)
    else: # free
        days_count = 14
        delta = datetime.timedelta(days=14)

    date_range = [current_date + datetime.timedelta(days=i) for i in range(days_count)]
    prev_date = (current_date - delta).strftime('%Y-%m-%d')
    next_date = (current_date + delta).strftime('%Y-%m-%d')

    # Ordering is CRITICAL for 'regroup' template tag
    if group_by == 'floor':
        order = ['floor', 'number']
    else:
        order = ['room_type__name_en', 'number']
        
    from django.db.models import Prefetch
    active_reservations = Reservation.objects.filter(check_in_date__lte=current_date, check_out_date__gt=current_date)
    rooms = Room.objects.all().select_related('room_type').prefetch_related(
        Prefetch('reservation_set', queryset=active_reservations, to_attr='active_res')
    ).order_by(*order)
    
    room_types = RoomType.objects.all()
    
    # Calculate Matrix Data
    matrix_data = []
    hotel_id = user.hotel_id or 1
    for rt in room_types:
        rt_dates = []
        total_rooms = Room.objects.filter(room_type=rt).count()
        for d in date_range:
            # For simplicity, we count reservations active on d
            # Real logic would check room status too
            res_count = Reservation.objects.filter(room__room_type=rt, hotel_id=hotel_id, check_in_date__lte=d, check_out_date__gt=d, is_paid=False).count()
            paid_count = Reservation.objects.filter(room__room_type=rt, hotel_id=hotel_id, check_in_date__lte=d, check_out_date__gt=d, is_paid=True).count()
            
            # Formula: Numerator = Total - (Res + Paid), Denom = Total - Res
            numerator = total_rooms - (res_count + paid_count)
            denominator = total_rooms - res_count
            
            # Color logic
            status = 'vacant'
            if (res_count + paid_count) >= total_rooms:
                status = 'sold' if paid_count >= (total_rooms - res_count) else 'reserved'
            
            rt_dates.append({
                'date': d,
                'numerator': numerator,
                'denominator': denominator,
                'status': status
            })
        matrix_data.append({
            'type': rt,
            'dates': rt_dates
        })

    context = {
        'rooms': rooms,
        'room_types': room_types,
        'view_type': view_type,
        'group_by': group_by,
        'range_type': range_type,
        'date_range': date_range,
        'current_date': current_date.strftime('%Y-%m-%d'),
        'prev_date': prev_date,
        'next_date': next_date,
        'matrix_data': matrix_data,
        'dirty_reserved_rooms': [room for room in rooms if room.status == 'DIRTY' and room.active_res and not room.active_res[0].is_paid and not room.active_res[0].is_occupied],
    }
    return render(request, 'rooms/overview.html', context)

@login_required
def guest_search_view(request):
    from django.http import JsonResponse
    from apps.guests.models import Guest
    q = request.GET.get('q', '')
    hotel_id = request.user.hotel_id or 1
    
    guests_query = Guest.objects.filter(hotel_id=hotel_id)
    if q:
        guests_query = guests_query.filter(
            models.Q(first_name__icontains=q) | 
            models.Q(last_name__icontains=q) | 
            models.Q(national_id__icontains=q) |
            models.Q(passport_number__icontains=q)
        )
    
    guests = guests_query.order_by('-id')[:50]
    
    data = [{
        'id': g.id,
        'first_name': g.first_name,
        'middle_name': g.middle_name,
        'last_name': g.last_name,
        'guest_number': g.guest_number,
        'phone': g.phone,
        'national_id': g.national_id,
        'email': g.email,
        'passport_number': g.passport_number
    } for g in guests]
    return JsonResponse(data, safe=False)

@login_required
def context_menu_action_view(request):
    from django.http import JsonResponse
    action = request.POST.get('action')
    room_id = request.POST.get('room_id')
    hotel_id = request.user.hotel_id or 1
    room = Room.objects.get(id=room_id, hotel_id=hotel_id)
    today = datetime.date.today()
    res = Reservation.objects.filter(room=room, check_in_date__lte=today, check_out_date__gt=today).first()

    if action == 'toggle_clean':
        room.status = 'CLEAN' if room.status == 'DIRTY' else 'DIRTY'
        room.save()
    elif action == 'check_in_out':
        if res:
            if not res.is_occupied:
                res.is_occupied = True
                res.check_in_time = datetime.datetime.now().time()
                room.status = 'OCCUPIED'
            else:
                res.is_occupied = False
                res.check_out_time = datetime.datetime.now().time()
                room.status = 'DIRTY'
            res.save()
            room.save()
    
    return JsonResponse({'success': True, 'status': room.status})

@login_required
def room_detail_view(request, room_id):
    from django.shortcuts import get_object_or_404, redirect
    from apps.guests.models import Guest
    from apps.reservations.models import ReservationGuest, Transaction
    from django.utils import timezone
    import math

    hotel_id = request.user.hotel_id or 1
    room = get_object_or_404(Room, id=room_id, hotel_id=hotel_id)
    today = datetime.date.today()
    tomorrow = today + datetime.timedelta(days=1)
    
    reservation = Reservation.objects.filter(room=room, check_in_date__lte=today, check_out_date__gt=today).first()
    
    if request.method == 'POST':
        action = request.POST.get('action')
        
        if action == 'save_reservation':
            # Get guest info from form
            f_name = request.POST.get('first_name'); m_name = request.POST.get('middle_name', ''); l_name = request.POST.get('last_name')
            phone = request.POST.get('phone'); n_id = request.POST.get('national_id'); email = request.POST.get('email')
            source = request.POST.get('source', 'FRONT_DESK'); check_in = request.POST.get('check_in_date'); check_out = request.POST.get('check_out_date')
            check_in_t = request.POST.get('check_in_time', '14:00'); check_out_t = request.POST.get('check_out_time', '12:00')
            
            guest, _ = Guest.objects.get_or_create(first_name=f_name, last_name=l_name, hotel_id=hotel_id, defaults={'middle_name': m_name, 'phone': phone, 'national_id': n_id, 'email': email})
            guest.middle_name = m_name; guest.phone = phone; guest.national_id = n_id; guest.email = email; guest.save()
            
            if not reservation:
                # No reservation yet — create one with this guest as primary
                reservation = Reservation.objects.create(room=room, guest=guest, check_in_date=check_in, check_out_date=check_out, check_in_time=check_in_t, check_out_time=check_out_t, hotel_id=hotel_id, source=source)
                ReservationGuest.objects.get_or_create(reservation=reservation, guest=guest, hotel_id=hotel_id, defaults={'is_primary': True})
            else:
                # Reservation exists — add this guest as a companion (if room has capacity)
                if reservation.guests.count() < room.capacity:
                    ReservationGuest.objects.get_or_create(reservation=reservation, guest=guest, hotel_id=hotel_id, defaults={'is_primary': False})
                # Also update reservation dates/source in case they changed
                reservation.check_in_date = check_in; reservation.check_out_date = check_out
                reservation.check_in_time = check_in_t; reservation.check_out_time = check_out_t
                reservation.source = source; reservation.save()
            return redirect(f"{request.path}?saved=1")

        elif action == 'add_companion':
            if reservation and reservation.guests.count() < room.capacity:
                f_name = request.POST.get('c_first_name'); l_name = request.POST.get('c_last_name'); n_id = request.POST.get('c_national_id')
                guest, _ = Guest.objects.get_or_create(first_name=f_name, last_name=l_name, hotel_id=hotel_id, defaults={'national_id': n_id})
                ReservationGuest.objects.create(reservation=reservation, guest=guest, hotel_id=hotel_id, is_primary=False)

        elif action == 'add_transaction':
            if reservation:
                amt = float(request.POST.get('amount', 0)); cur = request.POST.get('currency', 'USD'); rate = float(request.POST.get('rate', 1.0)); p_type = request.POST.get('p_type', 'Cash')
                Transaction.objects.create(reservation=reservation, amount_original=amt, currency=cur, conversion_rate=rate, amount_ref=amt * rate, payment_type=p_type, hotel_id=hotel_id)
                reservation.paid_amount = sum(t.amount_ref for t in reservation.transactions.all()); reservation.save()

        elif action == 'delete_transaction':
            if reservation:
                tr_id = request.POST.get('tr_id')
                Transaction.objects.filter(id=tr_id, reservation=reservation).delete()
                reservation.paid_amount = sum(t.amount_ref for t in reservation.transactions.all()); reservation.save()

        elif action == 'update_financials':
            if reservation:
                reservation.total_amount = float(request.POST.get('total_amount', 0))
                reservation.save()

        elif action == 'delete_guest':
            if reservation:
                guest_id = request.POST.get('guest_id')
                ReservationGuest.objects.filter(reservation=reservation, guest_id=guest_id, is_primary=False).delete()
        
        elif action == 'delete_reservation':
            if reservation:
                reservation.delete()
        
        elif action == 'check_action':
            if reservation:
                c_type = request.POST.get('type') # in/out
                now = timezone.now()
                if c_type == 'in':
                    reservation.is_occupied = True
                    reservation.actual_check_in = now
                    reservation.check_in_time = now.time()
                    room.status = 'OCCUPIED'
                else:
                    reservation.is_occupied = False
                    reservation.actual_check_out = now
                    reservation.check_out_time = now.time()
                    room.status = 'DIRTY'
                    if request.POST.get('cancel_future'):
                        reservation.check_out_date = now.date()
                reservation.save(); room.save()
        
        return redirect('room_detail', room_id=room.id)

    # Context calculations
    companions = reservation.guests.all() if reservation else []
    transactions = reservation.transactions.all().order_by('-id') if reservation else []
    daily_rate = 0
    days = 1
    if reservation:
        days = (reservation.check_out_date - reservation.check_in_date).days or 1
        daily_rate = math.floor(reservation.total_amount / days)

    balance_due = 0
    if reservation:
        balance_due = (reservation.total_amount or 0) - (reservation.paid_amount or 0)

    context = {
        'room': room, 'reservation': reservation, 'companions': companions, 'transactions': transactions, 'daily_rate': daily_rate,
        'balance_due': balance_due, 'nights': days,
        'default_check_in': today.strftime('%Y-%m-%d'), 'default_check_out': tomorrow.strftime('%Y-%m-%d'),
        'sources': ['front_desk', 'phone', 'booking', 'expedia'], 
        'currencies': ['TRY', 'USD', 'EUR', 'GBP', 'IRR'], 
        'p_types': ['cash', 'card', 'remittance']
    }
    return render(request, 'rooms/room_detail.html', context)
